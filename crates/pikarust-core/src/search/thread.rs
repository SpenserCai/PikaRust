use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;

use crate::nnue::Network;
use crate::position::rule_judge::RuleJudgeResult;
use crate::position::{GenType, Position, generate};
use crate::types::{Move, VALUE_INFINITE, VALUE_NONE, Value, is_decisive};

use super::search::{RootMove, Worker};
use super::time::SearchLimits;
use super::tt::TranspositionTable;

pub struct ThreadPool {
    workers: Vec<Worker>,
    handles: Vec<Option<thread::JoinHandle<Worker>>>,
    stop: Arc<AtomicBool>,
    ponder: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    tt: Arc<TranspositionTable>,
    increase_depth: Arc<AtomicBool>,
    tot_best_move_changes: Arc<AtomicU64>,
    worker_inbox: mpsc::Receiver<Vec<Worker>>,
    worker_return: mpsc::Sender<Vec<Worker>>,
    collector_handle: Option<thread::JoinHandle<()>>,
}

impl ThreadPool {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(num_threads: usize, tt_size_mb: usize, network: Option<Arc<Network>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let ponder = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(tt_size_mb));
        let increase_depth = Arc::new(AtomicBool::new(true));
        let tot_best_move_changes = Arc::new(AtomicU64::new(0));

        let mut workers = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            workers.push(Worker::new(
                i,
                Arc::clone(&stop),
                Arc::clone(&ponder),
                Arc::clone(&tt),
                Arc::clone(&increase_depth),
                Arc::clone(&tot_best_move_changes),
                num_threads,
                network.clone(),
            ));
        }

        let (worker_return, worker_inbox) = mpsc::channel();

        Self {
            workers,
            handles: Vec::new(),
            stop,
            ponder,
            failed: Arc::new(AtomicBool::new(false)),
            tt,
            increase_depth,
            tot_best_move_changes,
            worker_inbox,
            worker_return,
            collector_handle: None,
        }
    }

    pub fn clear(&mut self) {
        self.recover_workers();
        self.tt.clear();
        for w in &mut self.workers {
            w.clear();
        }
        if !self.workers.is_empty() {
            self.workers[0].best_previous_score = VALUE_INFINITE;
            self.workers[0].best_previous_avg_score = VALUE_INFINITE;
            self.workers[0].previous_time_reduction = 0.85;
            self.workers[0].calls_cnt = 0;
            self.workers[0].tm.clear();
        }
    }

    fn recover_workers(&mut self) {
        if self.collector_handle.is_some() || !self.handles.is_empty() {
            self.stop.store(true, Ordering::SeqCst);
        }
        if let Some(h) = self.collector_handle.take() {
            let _ = h.join();
        }
        while let Ok(workers) = self.worker_inbox.try_recv() {
            self.workers.extend(workers);
        }
        for handle in &mut self.handles {
            if let Some(h) = handle.take() {
                if let Ok(w) = h.join() {
                    self.workers.push(w);
                }
            }
        }
        self.handles.clear();
        self.workers.sort_by_key(|w| w.thread_idx);
    }

    pub fn start_search(&mut self, pos: &Position, limits: &SearchLimits) {
        self.recover_workers();

        // Each search owns its cancellation flags. Handles from a completed
        // search must never be able to cancel or resume a later search.
        self.stop = Arc::new(AtomicBool::new(false));
        self.ponder = Arc::new(AtomicBool::new(limits.ponder_mode));
        self.failed = Arc::new(AtomicBool::new(false));
        self.increase_depth.store(true, Ordering::SeqCst);
        self.tot_best_move_changes.store(0, Ordering::SeqCst);
        self.tt.new_search();

        let root_moves = build_root_moves(pos, limits);

        if root_moves.is_empty() {
            return;
        }

        for w in &mut self.workers {
            w.stop = Arc::clone(&self.stop);
            w.ponder = Arc::clone(&self.ponder);
            w.limits = limits.clone();
            w.nodes.store(0, Ordering::Relaxed);
            w.best_move_changes.store(0, Ordering::Relaxed);
            w.nmp_min_ply = 0;
            w.root_depth = 0;
            w.completed_depth = 0;
            w.root_moves.clone_from(&root_moves);
            w.root_pos = pos.clone();
        }

        let mut handles = Vec::new();
        let workers_to_spawn: Vec<Worker> = mem::take(&mut self.workers);

        for mut w in workers_to_spawn {
            let failed = Arc::clone(&self.failed);
            let handle = thread::spawn(move || {
                let stop = Arc::clone(&w.stop);
                let ponder = Arc::clone(&w.ponder);
                let tt = Arc::clone(&w.tt);
                let increase_depth = Arc::clone(&w.increase_depth);
                let tot_best_move_changes = Arc::clone(&w.tot_best_move_changes);
                let num_threads = w.num_threads;
                let network = w.network.clone();
                let thread_idx = w.thread_idx;

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    w.iterative_deepening();
                    w
                }));
                match result {
                    Ok(worker) => worker,
                    Err(payload) => {
                        let msg = payload
                            .downcast_ref::<&str>()
                            .map(|s| (*s).to_owned())
                            .or_else(|| payload.downcast_ref::<String>().cloned())
                            .unwrap_or_else(|| "unknown panic".to_owned());
                        log::error!("search worker {thread_idx} panicked: {msg}");
                        failed.store(true, Ordering::Relaxed);
                        stop.store(true, Ordering::SeqCst);
                        let mut recovered = Worker::new(
                            thread_idx,
                            stop,
                            ponder,
                            tt,
                            increase_depth,
                            tot_best_move_changes,
                            num_threads,
                            network,
                        );
                        recovered.clear();
                        recovered
                    }
                }
            });
            handles.push(Some(handle));
        }

        self.handles = handles;
    }

    pub fn start_search_async(
        &mut self,
        pos: &Position,
        limits: &SearchLimits,
    ) -> mpsc::Receiver<SearchResult> {
        self.start_search(pos, limits);

        let (tx, rx) = mpsc::sync_channel(1);
        let handles = mem::take(&mut self.handles);
        let worker_return_tx = self.worker_return.clone();
        let failed = Arc::clone(&self.failed);

        let collector = thread::spawn(move || {
            let mut workers = Vec::new();
            for h in handles.into_iter().flatten() {
                if let Ok(worker) = h.join() {
                    workers.push(worker);
                } else {
                    failed.store(true, Ordering::Relaxed);
                    log::error!("search thread join failed in collector");
                }
            }
            workers.sort_by_key(|w| w.thread_idx);

            // Joining establishes visibility of every worker's failure flag.
            // Recovered workers remain reusable, but their empty search state
            // must never be presented as a successful result. Dropping `tx`
            // reports a disconnected search to the checked handle API.
            let result = (!failed.load(Ordering::Relaxed)).then(|| extract_search_result(&workers));
            let _ = worker_return_tx.send(workers);
            if let Some(result) = result {
                let _ = tx.send(result);
            }
        });

        self.collector_handle = Some(collector);
        rx
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub const fn ponder_flag(&self) -> &Arc<AtomicBool> {
        &self.ponder
    }

    pub const fn stop_flag(&self) -> &Arc<AtomicBool> {
        &self.stop
    }

    #[cfg(test)]
    pub(crate) fn inject_worker_failure(&mut self) {
        // Exercise the real panic/recovery path without a model fixture or a
        // production failure hook. Search always indexes this stack.
        self.workers[0].ss_static_evals.clear();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.collector_handle.take() {
            let _ = h.join();
        }
        while let Ok(workers) = self.worker_inbox.try_recv() {
            self.workers.extend(workers);
        }
        for handle in &mut self.handles {
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}

/// Result extracted from completed search workers.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub best_move: Move,
    pub ponder_move: Option<Move>,
    pub score: Value,
    pub depth: i32,
    pub seldepth: i32,
    pub nodes: u64,
    pub hashfull: i32,
    pub pv: Vec<Move>,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            best_move: Move::NONE,
            ponder_move: None,
            score: -VALUE_INFINITE,
            depth: 0,
            seldepth: 0,
            nodes: 0,
            hashfull: 0,
            pv: Vec::new(),
        }
    }
}

fn extract_search_result(workers: &[Worker]) -> SearchResult {
    if workers.is_empty() {
        return SearchResult::default();
    }

    let best_idx = find_best_thread_idx(workers);
    let nodes: u64 = workers.iter().map(Worker::node_count).sum();
    let best_move = if workers[best_idx].root_moves.is_empty() {
        Move::NONE
    } else {
        workers[best_idx].root_moves[0].pv[0]
    };
    let score = if workers[best_idx].root_moves.is_empty() {
        -VALUE_INFINITE
    } else {
        workers[best_idx].root_moves[0].score
    };
    let depth = workers[best_idx].completed_depth;
    let mut pv = workers[best_idx]
        .root_moves
        .first()
        .map_or_else(Vec::new, |root| root.pv.clone());
    if pv.len() == 1 {
        pv.extend(ponder_from_tt(&workers[best_idx], best_move));
    }
    let ponder_move = pv.get(1).copied();

    SearchResult {
        best_move,
        ponder_move,
        score,
        depth,
        seldepth: workers[best_idx].sel_depth,
        nodes,
        hashfull: workers[best_idx]
            .tt
            .hashfull(workers[best_idx].tt.generation()),
        pv,
    }
}

fn ponder_from_tt(worker: &Worker, best_move: Move) -> Option<Move> {
    // Match upstream RootMove::extract_ponder_from_tt after search completes.
    // A private position copy keeps worker history, search counters, and the
    // root position unchanged on every return path, including rule adjudication.
    let mut position = worker.root_pos.clone();
    if !position.is_legal_move(best_move) {
        return None;
    }
    let gives_check = position.gives_check(best_move);
    position.do_move(best_move, gives_check);
    if matches!(position.rule_judge(1), RuleJudgeResult::Definitive(_)) {
        return None;
    }
    let probe = worker.tt.probe(position.key());
    (probe.found && position.is_legal_move(probe.data.tt_move)).then_some(probe.data.tt_move)
}

fn find_best_thread_idx(workers: &[Worker]) -> usize {
    if workers.is_empty() {
        return 0;
    }

    let mut min_score = VALUE_NONE;
    for w in workers {
        if !w.root_moves.is_empty() {
            min_score = min_score.min(w.root_moves[0].score);
        }
    }

    let mut votes: std::collections::HashMap<u16, i64> = std::collections::HashMap::new();

    let voting_value = |w: &Worker| -> i64 {
        if w.root_moves.is_empty() {
            return 0;
        }
        i64::from(w.root_moves[0].score - min_score + 14) * i64::from(w.completed_depth)
    };

    for w in workers {
        if !w.root_moves.is_empty() {
            let key = w.root_moves[0].pv[0].raw();
            *votes.entry(key).or_insert(0) += voting_value(w);
        }
    }

    let mut best_idx = 0;
    let mut best_voting = i64::MIN;

    for (i, w) in workers.iter().enumerate() {
        if w.root_moves.is_empty() {
            continue;
        }
        let key = w.root_moves[0].pv[0].raw();
        let v = votes.get(&key).copied().unwrap_or(0);

        let score = w.root_moves[0].score;
        let is_decisive_score = score != -VALUE_INFINITE && is_decisive(score);

        if is_decisive_score {
            if i == best_idx
                || !is_decisive(workers[best_idx].root_moves[0].score)
                || score.abs() > workers[best_idx].root_moves[0].score.abs()
            {
                best_idx = i;
            }
        } else if !is_decisive(workers[best_idx].root_moves[0].score)
            && (v > best_voting
                || (v == best_voting && voting_value(w) > voting_value(&workers[best_idx])))
        {
            best_idx = i;
            best_voting = v;
        }
    }

    best_idx
}

fn build_root_moves(pos: &Position, limits: &SearchLimits) -> Vec<RootMove> {
    let legal_moves = generate(pos, GenType::Legal);
    let mut root_moves = Vec::new();

    if limits.search_moves.is_empty() {
        for i in 0..legal_moves.len() {
            root_moves.push(RootMove::new(legal_moves.get(i)));
        }
    } else {
        for i in 0..legal_moves.len() {
            let m = legal_moves.get(i);
            let m_str = format!("{m}");
            if limits.search_moves.iter().any(|sm| sm == &m_str) {
                root_moves.push(RootMove::new(m));
            }
        }
    }

    root_moves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Bound, Piece, PieceType, Square};
    use std::time::Duration;

    fn history_biases(worker: &Worker) -> [i16; 4] {
        [
            worker
                .capture_history
                .get(Piece::W_ROOK, Square::SQ_A1, PieceType::Pawn),
            worker
                .continuation_history
                .get(false, false, Piece::W_KNIGHT, Square::SQ_B0)
                .get(Piece::W_PAWN, Square::SQ_A3),
            worker
                .pawn_history
                .entry(0)
                .get(Piece::W_PAWN, Square::SQ_A3),
            worker
                .continuation_correction_history
                .get(Piece::W_KNIGHT, Square::SQ_B0)
                .get(Piece::W_PAWN, Square::SQ_A3),
        ]
    }

    #[test]
    fn recovered_worker_matches_canonical_initialization() {
        let mut recovered = ThreadPool::new(1, 1, None);
        let mut fresh = ThreadPool::new(1, 1, None);
        recovered.clear();
        fresh.clear();
        let pos = Position::start_pos().unwrap();
        let mut limits = SearchLimits::new();
        limits.depth = 3;

        recovered.inject_worker_failure();
        let failed = recovered.start_search_async(&pos, &limits);
        assert!(matches!(
            failed.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        recovered.recover_workers();
        assert_eq!(recovered.workers.len(), 1);
        assert_eq!(
            history_biases(&recovered.workers[0]),
            history_biases(&fresh.workers[0])
        );

        // Remove entries written before the injected failure, so the search
        // comparison isolates worker initialization rather than cache warmth.
        recovered.tt.clear();
        let actual = recovered
            .start_search_async(&pos, &limits)
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let expected = fresh
            .start_search_async(&pos, &limits)
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(actual.best_move, expected.best_move);
        assert_eq!(actual.score, expected.score);
        assert_eq!(actual.depth, expected.depth);
        assert_eq!(actual.nodes, expected.nodes);
        assert_eq!(actual.pv, expected.pv);
    }

    fn store_ponder(pool: &ThreadPool, best_move: Move, ponder: Move) {
        let mut child = pool.workers[0].root_pos.clone();
        assert!(child.is_legal_move(best_move));
        let gives_check = child.gives_check(best_move);
        child.do_move(best_move, gives_check);
        pool.tt.probe(child.key()).writer.write(
            &pool.tt,
            child.key(),
            0,
            false,
            Bound::Lower,
            1,
            ponder,
            0,
            pool.tt.generation(),
        );
    }

    #[test]
    fn ponder_fallback_matches_reference_bench_42_without_mutating_search_state() {
        // Pinned Pikafish bench 42 at depth 5 finishes with a one-move root PV
        // and then adds g0e2 from the child position's TT entry for final output.
        let mut pool = ThreadPool::new(1, 1, None);
        pool.workers[0].root_pos =
            Position::from_fen("4k1b2/4a4/5a3/6P1C/9/p4Nn2/2n6/9/4K4/5AB2 b - - 0 1").unwrap();
        let best_move = Move::make(Square::SQ_G4, Square::SQ_H2);
        let ponder = Move::make(Square::SQ_G0, Square::SQ_E2);
        pool.workers[0].root_moves.push(RootMove::new(best_move));
        pool.workers[0].root_moves[0].score = 9;
        pool.workers[0].completed_depth = 5;
        pool.workers[0].nodes.store(612, Ordering::Relaxed);

        let before = extract_search_result(&pool.workers);
        assert_eq!(before.pv, [best_move]);
        assert_eq!(before.ponder_move, None);
        let root_fen = pool.workers[0].root_pos.fen();
        let root_key = pool.workers[0].root_pos.key();
        store_ponder(&pool, best_move, ponder);

        let result = extract_search_result(&pool.workers);
        assert_eq!(result.pv, [best_move, ponder]);
        assert_eq!(result.ponder_move, Some(ponder));
        assert_eq!(result.best_move, before.best_move);
        assert_eq!(result.score, before.score);
        assert_eq!(result.depth, before.depth);
        assert_eq!(result.nodes, before.nodes);
        assert_eq!(pool.workers[0].root_pos.fen(), root_fen);
        assert_eq!(pool.workers[0].root_pos.key(), root_key);
        assert_eq!(pool.workers[0].root_moves[0].pv, [best_move]);

        // An existing searched continuation takes precedence over the TT hint.
        pool.workers[0].root_moves[0].pv.push(ponder);
        store_ponder(&pool, best_move, Move::make(Square::SQ_G0, Square::SQ_I2));
        assert_eq!(extract_search_result(&pool.workers).pv, [best_move, ponder]);
    }

    #[test]
    fn ponder_fallback_rejects_illegal_tt_moves() {
        let mut pool = ThreadPool::new(1, 1, None);
        pool.workers[0].root_pos = Position::start_pos().unwrap();
        let best_move = Move::make(Square::SQ_B0, Square::SQ_C2);
        pool.workers[0].root_moves.push(RootMove::new(best_move));
        for invalid in [
            Move::NONE,
            Move::NULL,
            Move::from_raw(u16::MAX),
            Move::make(Square::SQ_B9, Square::SQ_B4),
            Move::make(Square::SQ_H0, Square::SQ_G2),
        ] {
            pool.tt.clear();
            store_ponder(&pool, best_move, invalid);
            let result = extract_search_result(&pool.workers);
            assert_eq!(result.pv, [best_move], "raw move {}", invalid.raw());
            assert_eq!(result.ponder_move, None);
        }
    }

    #[test]
    fn ponder_fallback_does_not_extend_a_definitive_result() {
        let mut pool = ThreadPool::new(1, 1, None);
        pool.workers[0].root_pos = Position::from_fen(
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 119 1",
        )
        .unwrap();
        let best_move = Move::make(Square::SQ_B0, Square::SQ_C2);
        let ponder = Move::make(Square::SQ_B9, Square::SQ_C7);
        pool.workers[0].root_moves.push(RootMove::new(best_move));
        store_ponder(&pool, best_move, ponder);

        let mut child = pool.workers[0].root_pos.clone();
        let gives_check = child.gives_check(best_move);
        child.do_move(best_move, gives_check);
        assert!(child.is_legal_move(ponder));
        assert!(matches!(
            child.rule_judge(1),
            RuleJudgeResult::Definitive(_)
        ));
        let result = extract_search_result(&pool.workers);
        assert_eq!(result.pv, [best_move]);
        assert_eq!(result.ponder_move, None);
        assert_eq!(pool.workers[0].root_pos.rule60_count(), 119);
    }
}
