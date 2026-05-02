use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    tt: Arc<TranspositionTable>,
    increase_depth: Arc<AtomicBool>,
    tot_best_move_changes: Arc<AtomicU64>,
}

impl ThreadPool {
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(num_threads: usize, tt_size_mb: usize, network: Option<Arc<Network>>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(tt_size_mb));
        let increase_depth = Arc::new(AtomicBool::new(true));
        let tot_best_move_changes = Arc::new(AtomicU64::new(0));

        let mut workers = Vec::with_capacity(num_threads);
        for i in 0..num_threads {
            workers.push(Worker::new(
                i,
                Arc::clone(&stop),
                Arc::clone(&tt),
                Arc::clone(&increase_depth),
                Arc::clone(&tot_best_move_changes),
                num_threads,
                network.clone(),
            ));
        }

        Self {
            workers,
            handles: Vec::new(),
            stop,
            tt,
            increase_depth,
            tot_best_move_changes,
        }
    }

    pub fn clear(&mut self) {
        for w in &mut self.workers {
            w.clear();
        }
        self.workers[0].best_previous_score = VALUE_INFINITE;
        self.workers[0].best_previous_avg_score = VALUE_INFINITE;
        self.workers[0].previous_time_reduction = 0.85;
        self.workers[0].calls_cnt = 0;
        self.workers[0].tm.clear();
    }

    pub fn start_search(&mut self, pos: &Position, limits: &SearchLimits) {
        self.wait_for_search();

        self.stop.store(false, Ordering::SeqCst);
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
                // Clone Arc references before moving `w` into catch_unwind,
                // so we can reconstruct a worker if the closure panics.
                let stop = Arc::clone(&w.stop);
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
                        // Signal stop so other threads wind down
                        stop.store(true, Ordering::SeqCst);
                        // Return a fresh worker sharing the same Arcs
                        Worker::new(
                            thread_idx,
                            stop,
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

    pub fn wait_for_search(&mut self) {
        for handle in &mut self.handles {
            if let Some(h) = handle.take() {
                match h.join() {
                    Ok(w) => self.workers.push(w),
                    Err(_) => {
                        // This should not happen since we use catch_unwind,
                        // but log it defensively.
                        log::error!("search thread join failed unexpectedly");
                    }
                }
            }
        }
        self.handles.clear();
        self.workers.sort_by_key(|w| w.thread_idx);
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn nodes_searched(&self) -> u64 {
        self.workers.iter().map(Worker::node_count).sum()
    }

    pub fn best_thread_idx(&self) -> usize {
        if self.workers.is_empty() {
            return 0;
        }

        let mut min_score = VALUE_NONE;
        for w in &self.workers {
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

        for w in &self.workers {
            if !w.root_moves.is_empty() {
                let key = w.root_moves[0].pv[0].raw();
                *votes.entry(key).or_insert(0) += voting_value(w);
            }
        }

        let mut best_idx = 0;
        let mut best_voting = i64::MIN;

        for (i, w) in self.workers.iter().enumerate() {
            if w.root_moves.is_empty() {
                continue;
            }
            let key = w.root_moves[0].pv[0].raw();
            let v = votes.get(&key).copied().unwrap_or(0);

            let score = w.root_moves[0].score;
            let is_decisive_score = score != -VALUE_INFINITE && is_decisive(score);

            if is_decisive_score {
                if i == best_idx
                    || !is_decisive(self.workers[best_idx].root_moves[0].score)
                    || score.abs() > self.workers[best_idx].root_moves[0].score.abs()
                {
                    best_idx = i;
                }
            } else if !is_decisive(self.workers[best_idx].root_moves[0].score)
                && (v > best_voting
                    || (v == best_voting
                        && voting_value(w) > voting_value(&self.workers[best_idx])))
            {
                best_idx = i;
                best_voting = v;
            }
        }

        best_idx
    }

    pub fn best_move(&self) -> Option<Move> {
        let idx = self.best_thread_idx();
        if idx < self.workers.len() && !self.workers[idx].root_moves.is_empty() {
            Some(self.workers[idx].root_moves[0].pv[0])
        } else {
            None
        }
    }

    pub fn best_score(&self) -> Value {
        let idx = self.best_thread_idx();
        if idx < self.workers.len() && !self.workers[idx].root_moves.is_empty() {
            self.workers[idx].root_moves[0].score
        } else {
            -VALUE_INFINITE
        }
    }

    pub fn worker(&self, idx: usize) -> &Worker {
        &self.workers[idx]
    }

    pub fn worker_mut(&mut self, idx: usize) -> &mut Worker {
        &mut self.workers[idx]
    }

    pub fn num_threads(&self) -> usize {
        self.workers.len()
    }

    pub fn tt(&self) -> &TranspositionTable {
        &self.tt
    }

    pub const fn new_search(&self) {
        // tt.new_search() is called in start_search
    }
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
