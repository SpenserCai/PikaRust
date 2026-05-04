use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;

use crate::nnue::Network;
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

        self.stop.store(false, Ordering::SeqCst);
        self.ponder.store(limits.ponder_mode, Ordering::SeqCst);
        self.increase_depth.store(true, Ordering::SeqCst);
        self.tot_best_move_changes.store(0, Ordering::SeqCst);
        if let Some(tt) = Arc::get_mut(&mut self.tt) {
            tt.new_search();
        }

        let root_moves = build_root_moves(pos, limits);

        if root_moves.is_empty() {
            return;
        }

        for w in &mut self.workers {
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
        let workers_to_spawn: Vec<Worker> = self.workers.drain(..).collect();

        for mut w in workers_to_spawn {
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
                        stop.store(true, Ordering::SeqCst);
                        Worker::new(
                            thread_idx,
                            stop,
                            ponder,
                            tt,
                            increase_depth,
                            tot_best_move_changes,
                            num_threads,
                            network,
                        )
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

        let collector = thread::spawn(move || {
            let mut workers = Vec::new();
            for h in handles.into_iter().flatten() {
                match h.join() {
                    Ok(w) => workers.push(w),
                    Err(_) => {
                        log::error!("search thread join failed in collector");
                    }
                }
            }
            workers.sort_by_key(|w| w.thread_idx);

            let result = extract_search_result(&workers);
            let _ = worker_return_tx.send(workers);
            let _ = tx.send(result);
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
    let ponder_move = if best_move != Move::NONE
        && !workers[best_idx].root_moves.is_empty()
        && workers[best_idx].root_moves[0].pv.len() > 1
    {
        Some(workers[best_idx].root_moves[0].pv[1])
    } else {
        None
    };

    SearchResult {
        best_move,
        ponder_move,
        score,
        depth,
        seldepth: workers[best_idx].sel_depth,
        nodes,
        hashfull: workers[best_idx].tt.hashfull(workers[best_idx].tt.generation()),
        pv: if workers[best_idx].root_moves.is_empty() {
            Vec::new()
        } else {
            workers[best_idx].root_moves[0].pv.clone()
        },
    }
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
