// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// Search/evaluation aligned with Pikafish b562d6ae, 2026-09-28.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::nnue::features::half_ka_v2_hm;
use crate::nnue::{AccumulatorStack, DiffType, DirtyPiece, DirtyThreats, Network};
use crate::position::Position;
use crate::types::{
    Color, Depth, MAX_PLY, Move, Piece, PieceType, Square, VALUE_DRAW, VALUE_INFINITE,
    VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY, VALUE_NONE, VALUE_ZERO, Value, is_valid,
};

use super::evaluate;
use super::history::{
    ButterflyHistory, CapturePieceToHistory, ContHistIndex, ContinuationCorrectionHistory,
    ContinuationHistory, LowPlyHistory, PawnHistory, TTMoveHistory, UnifiedCorrectionHistory,
};
use super::time::{SearchLimits, TimeManager};
use super::tt::TranspositionTable;

pub fn lmr_divisor(depth: Depth) -> i32 {
    let d = depth.min(16);
    3000 + 7 * (d - 8) * (d - 8)
}

pub const SEARCHED_LIST_CAPACITY: usize = 32;
const SS_OFFSET: usize = 7;

pub struct PVLine {
    pub moves: Vec<Move>,
}

impl PVLine {
    pub fn new() -> Self {
        Self {
            moves: Vec::with_capacity(MAX_PLY as usize),
        }
    }

    pub fn clear(&mut self) {
        self.moves.clear();
    }

    pub fn update(&mut self, m: Move, child: &Self) {
        self.moves.clear();
        self.moves.push(m);
        self.moves.extend_from_slice(&child.moves);
    }
}

impl Default for PVLine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct RootMove {
    pub score: Value,
    pub previous_score: Value,
    pub average_score: Value,
    pub mean_squared_score: i64,
    pub uci_score: Value,
    pub score_lowerbound: bool,
    pub score_upperbound: bool,
    pub sel_depth: i32,
    pub effort: u64,
    pub pv: Vec<Move>,
    pub previous_pv: Vec<Move>,
}

impl RootMove {
    pub fn new(m: Move) -> Self {
        Self {
            score: -VALUE_INFINITE,
            previous_score: -VALUE_INFINITE,
            average_score: -VALUE_INFINITE,
            mean_squared_score: -i64::from(VALUE_INFINITE) * i64::from(VALUE_INFINITE),
            uci_score: -VALUE_INFINITE,
            score_lowerbound: false,
            score_upperbound: false,
            sel_depth: 0,
            effort: 0,
            pv: vec![m],
            previous_pv: Vec::new(),
        }
    }

    pub const fn is_inexact(&self) -> bool {
        self.score_lowerbound || self.score_upperbound
    }

    pub const fn is_exact_loss(&self) -> bool {
        self.score != -VALUE_INFINITE && crate::types::is_loss(self.score) && !self.is_inexact()
    }

    pub const fn unset_inexact(&mut self) {
        self.score_lowerbound = false;
        self.score_upperbound = false;
    }

    /// Effort-weighted averages with the upstream unsigned intermediate arithmetic.
    pub(super) fn update_average(&mut self, value: Value, nodes: u64) {
        let previous_effort = self.effort.max(1);
        self.effort += nodes;
        let weight = (64 * nodes / (2 * nodes + 3 * previous_effort)).clamp(12, 24);
        let squared_weight = weight.min(16);
        let squared = i64::from(value) * i64::from(value.abs());
        self.average_score = if self.average_score == -VALUE_INFINITE {
            value
        } else {
            weighted_score(i64::from(value), i64::from(self.average_score), weight)
        };
        self.mean_squared_score =
            if self.mean_squared_score == -i64::from(VALUE_INFINITE) * i64::from(VALUE_INFINITE) {
                squared
            } else {
                i64::from(weighted_score(
                    squared,
                    self.mean_squared_score,
                    squared_weight,
                ))
            };
    }
}

const fn weighted_score(value: i64, previous: i64, weight: u64) -> Value {
    // C++ multiplies signed scores by u64 weights, divides the wrapping unsigned
    // sum, then converts to Value. Signed division would round negative scores
    // differently and change subsequent aspiration windows.
    ((value as u64)
        .wrapping_mul(weight)
        .wrapping_add((previous as u64).wrapping_mul(32 - weight))
        / 32) as Value
}

pub struct Worker {
    pub thread_idx: usize,
    pub root_pos: Position,
    pub root_moves: Vec<RootMove>,
    pub root_depth: Depth,
    pub completed_depth: Depth,
    pub sel_depth: i32,
    pub nodes: AtomicU64,
    pub best_move_changes: AtomicU64,
    pub nmp_min_ply: i32,
    pub root_delta: Value,
    pub optimism: [Value; Color::NUM],
    pub pv_idx: usize,
    pub pv_last: usize,
    pub limits: SearchLimits,
    pub last_iteration_pv: Vec<Move>,

    pub main_history: ButterflyHistory,
    pub low_ply_history: LowPlyHistory,
    pub capture_history: CapturePieceToHistory,
    pub continuation_history: Arc<ContinuationHistory>,
    pub pawn_history: PawnHistory,
    pub tt_move_history: TTMoveHistory,
    pub correction_history: UnifiedCorrectionHistory,
    pub continuation_correction_history: ContinuationCorrectionHistory,

    pub reductions: [i32; MAX_PLY as usize + 10],

    pub stop: Arc<AtomicBool>,
    pub ponder: Arc<AtomicBool>,
    pub tt: Arc<TranspositionTable>,
    pub increase_depth: Arc<AtomicBool>,
    pub tot_best_move_changes: Arc<AtomicU64>,
    pub num_threads: usize,

    pub network: Option<Arc<Network>>,

    pub acc_stack: AccumulatorStack,

    pub tm: TimeManager,
    pub best_previous_score: Value,
    pub best_previous_avg_score: Value,
    pub previous_time_reduction: f64,
    pub stop_on_ponderhit: bool,
    pub calls_cnt: i32,
    pub iter_value: [Value; 4],

    // Per-ply search stack data stored as parallel arrays
    pub ss_static_evals: Vec<Value>,
    pub ss_tt_pvs: Vec<bool>,
    pub ss_excluded_moves: Vec<Move>,
    pub ss_in_check: Vec<bool>,
    pub ss_current_moves: Vec<Move>,
    pub ss_move_counts: Vec<i32>,
    pub ss_cutoff_cnts: Vec<i32>,
    pub ss_reductions: Vec<i32>,
    pub ss_stat_scores: Vec<i32>,
    pub ss_follow_pvs: Vec<bool>,
    pub ss_cont_hist_indices: Vec<ContHistIndex>,
    pub ss_tt_hits: Vec<bool>,
    pub ss_prior_nmp_fail_high: Vec<i32>,
    /// Per-ply PV arrays for PV propagation (matches Pikafish ss->pv).
    pub ss_pvs: Vec<Vec<Move>>,
}

impl Worker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        thread_idx: usize,
        stop: Arc<AtomicBool>,
        ponder: Arc<AtomicBool>,
        tt: Arc<TranspositionTable>,
        increase_depth: Arc<AtomicBool>,
        tot_best_move_changes: Arc<AtomicU64>,
        num_threads: usize,
        network: Option<Arc<Network>>,
    ) -> Self {
        Self::with_continuation_history(
            thread_idx,
            stop,
            ponder,
            tt,
            increase_depth,
            tot_best_move_changes,
            num_threads,
            network,
            Arc::new(ContinuationHistory::new()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn with_continuation_history(
        thread_idx: usize,
        stop: Arc<AtomicBool>,
        ponder: Arc<AtomicBool>,
        tt: Arc<TranspositionTable>,
        increase_depth: Arc<AtomicBool>,
        tot_best_move_changes: Arc<AtomicU64>,
        num_threads: usize,
        network: Option<Arc<Network>>,
        continuation_history: Arc<ContinuationHistory>,
    ) -> Self {
        let ss_size = MAX_PLY as usize + 10;
        let mut w = Self {
            thread_idx,
            root_pos: Position::new(),
            root_moves: Vec::new(),
            root_depth: 0,
            completed_depth: 0,
            sel_depth: 0,
            nodes: AtomicU64::new(0),
            best_move_changes: AtomicU64::new(0),
            nmp_min_ply: 0,
            root_delta: VALUE_INFINITE,
            optimism: [0; Color::NUM],
            pv_idx: 0,
            pv_last: 0,
            limits: SearchLimits::new(),
            last_iteration_pv: Vec::new(),

            main_history: ButterflyHistory::new(),
            low_ply_history: LowPlyHistory::new(),
            capture_history: CapturePieceToHistory::new(),
            continuation_history,
            pawn_history: PawnHistory::new(),
            tt_move_history: TTMoveHistory::new(),
            correction_history: UnifiedCorrectionHistory::new(1),
            continuation_correction_history: ContinuationCorrectionHistory::new(),

            reductions: [0; MAX_PLY as usize + 10],

            stop,
            ponder,
            tt,
            increase_depth,
            tot_best_move_changes,
            num_threads,

            network,

            acc_stack: AccumulatorStack::new(MAX_PLY as usize + 10),

            tm: TimeManager::new(),
            best_previous_score: VALUE_INFINITE,
            best_previous_avg_score: VALUE_INFINITE,
            previous_time_reduction: 0.85,
            stop_on_ponderhit: false,
            calls_cnt: 0,
            iter_value: [VALUE_ZERO; 4],

            ss_static_evals: vec![VALUE_NONE; ss_size],
            ss_tt_pvs: vec![false; ss_size],
            ss_excluded_moves: vec![Move::NONE; ss_size],
            ss_in_check: vec![false; ss_size],
            ss_current_moves: vec![Move::NONE; ss_size],
            ss_move_counts: vec![0; ss_size],
            ss_cutoff_cnts: vec![0; ss_size],
            ss_reductions: vec![0; ss_size],
            ss_stat_scores: vec![0; ss_size],
            ss_follow_pvs: vec![false; ss_size],
            ss_cont_hist_indices: vec![ContHistIndex::SENTINEL; ss_size],
            ss_tt_hits: vec![false; ss_size],
            ss_prior_nmp_fail_high: vec![0; ss_size],
            ss_pvs: vec![Vec::new(); ss_size],
        };
        w.init_reductions();
        w
    }

    fn init_reductions(&mut self) {
        for i in 1..self.reductions.len() {
            self.reductions[i] = (1713.0 / 100.0 * (i as f64).ln()) as i32;
        }
    }

    pub fn clear(&mut self) {
        self.main_history.fill(-5);
        self.stop_on_ponderhit = false;
        self.capture_history.fill(-604);
        self.low_ply_history.fill(0);
        self.continuation_history.fill(-440);
        self.pawn_history.fill(-1262);
        self.tt_move_history.reset();
        self.correction_history.fill(-6);
        self.continuation_correction_history.fill(7);
        self.init_reductions();
    }

    pub const fn is_main_thread(&self) -> bool {
        self.thread_idx == 0
    }

    #[inline]
    pub fn inc_nodes(&self) {
        self.nodes
            .store(self.nodes.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
    }

    #[inline]
    pub fn node_count(&self) -> u64 {
        self.nodes.load(Ordering::Relaxed)
    }

    pub fn reduction(&self, improving: bool, d: Depth, mn: i32, delta: i32) -> i32 {
        let d_idx = (d as usize).min(self.reductions.len() - 1);
        let mn_idx = (mn as usize).min(self.reductions.len() - 1);
        let reduction_scale = self.reductions[d_idx] * self.reductions[mn_idx];
        let root_delta = self.root_delta.max(1);
        reduction_scale - delta * 1128 / root_delta
            + if improving {
                0
            } else {
                reduction_scale * 165 / 512
            }
            + 1931
    }

    pub fn evaluate_pos(&mut self) -> Value {
        let optimism = self.optimism[self.root_pos.side_to_move().index()];
        let Some(net) = self.network.as_ref() else {
            return evaluate::evaluate_simple(&self.root_pos, optimism);
        };

        let psq_computed = self.acc_stack.current_psq().acc.computed[0]
            && self.acc_stack.current_psq().acc.computed[1];

        if !psq_computed {
            if let Some((prev, current)) = self.acc_stack.prev_and_current_psq_mut() {
                if prev.acc.computed[0] && prev.acc.computed[1] {
                    if let DiffType::DirtyPiece(ref dirty) = current.diff {
                        let dirty_copy = dirty.clone(); // 24 bytes, not 4.2KB
                        crate::nnue::feature_transformer::update_psq_accumulator_incremental(
                            net.model(),
                            &self.root_pos,
                            &prev.acc,
                            &mut current.acc,
                            &dirty_copy,
                            net.simd(),
                        );
                    } else {
                        crate::nnue::feature_transformer::refresh_psq_accumulator(
                            net.model(),
                            &self.root_pos,
                            &mut current.acc,
                            net.simd(),
                        );
                    }
                } else {
                    crate::nnue::feature_transformer::refresh_psq_accumulator(
                        net.model(),
                        &self.root_pos,
                        &mut current.acc,
                        net.simd(),
                    );
                }
            } else {
                let acc = &mut self.acc_stack.current_psq_mut().acc;
                crate::nnue::feature_transformer::refresh_psq_accumulator(
                    net.model(),
                    &self.root_pos,
                    acc,
                    net.simd(),
                );
            }
        }

        let threat_computed = self.acc_stack.current_threat().acc.computed[0]
            && self.acc_stack.current_threat().acc.computed[1];
        if !threat_computed {
            let acc = &mut self.acc_stack.current_threat_mut().acc;
            crate::nnue::feature_transformer::refresh_threat_accumulator(
                net.model(),
                &self.root_pos,
                acc,
                net.simd(),
            );
        }

        let psq_acc = &self.acc_stack.current_psq().acc;
        let threat_acc = &self.acc_stack.current_threat().acc;

        let (nnue_psqt, nnue_positional) = net.evaluate(
            &psq_acc.accumulation,
            &threat_acc.accumulation,
            &psq_acc.psqt_accumulation,
            &threat_acc.psqt_accumulation,
            &self.root_pos.piece_count,
            self.root_pos.side_to_move(),
        );
        evaluate::evaluate(&self.root_pos, nnue_psqt, nnue_positional, optimism)
    }

    pub fn value_draw(&self) -> Value {
        VALUE_DRAW - 1 + (self.node_count() & 0x2) as Value
    }

    pub fn push_acc_for_move(&mut self, m: Move, gives_check: bool) {
        let from = m.from_sq();
        let to = m.to_sq();
        let pc = self.root_pos.piece_on(from);
        let captured = self.root_pos.piece_on(to);

        let attack_bucket_before = [
            half_ka_v2_hm::make_attack_bucket(&self.root_pos, Color::White),
            half_ka_v2_hm::make_attack_bucket(&self.root_pos, Color::Black),
        ];

        let mut dirty = DirtyPiece::new();
        dirty.pc[0] = pc;
        dirty.from[0] = from;
        dirty.to[0] = to;

        if captured == Piece::NONE {
            dirty.dirty_num = 1;
        } else {
            dirty.dirty_num = 2;
            dirty.pc[1] = captured;
            dirty.from[1] = to;
            dirty.to[1] = Square::NONE;
        }

        let us = self.root_pos.side_to_move();
        if pc.piece_type() == PieceType::King {
            dirty.requires_refresh[0] = true;
            dirty.requires_refresh[1] = true;
        }

        if captured != Piece::NONE {
            let cpt = captured.piece_type();
            if cpt == PieceType::Rook || cpt == PieceType::Knight || cpt == PieceType::Cannon {
                let them = !us;
                let new_attack_bucket = {
                    let rook_count = self.root_pos.count_type(them, PieceType::Rook)
                        - u8::from(cpt == PieceType::Rook);
                    let kc_count = self.root_pos.count_type(them, PieceType::Knight)
                        + self.root_pos.count_type(them, PieceType::Cannon)
                        - u8::from(cpt == PieceType::Knight || cpt == PieceType::Cannon);
                    u32::from(rook_count > 0) * 2 + u32::from(kc_count > 0)
                };
                if new_attack_bucket != attack_bucket_before[them as usize] {
                    dirty.requires_refresh[them as usize] = true;
                }
            }
        }

        // Pikafish: mirror_before[0] = us perspective, mirror_before[1] = them perspective
        let them = !us;
        let mirror_before_us = half_ka_v2_hm::make_feature_bucket(us, &self.root_pos).1;
        let mirror_before_them = half_ka_v2_hm::make_feature_bucket(them, &self.root_pos).1;

        // Do the move (with threat diff computation)
        let mut dts = DirtyThreats::new();
        self.root_pos.do_move_with_threats(m, gives_check, &mut dts);

        // Pikafish: dp.requires_refresh[c] |= (mirror_before[c] != mirror_after[c])
        let mirror_after_us = half_ka_v2_hm::make_feature_bucket(us, &self.root_pos).1;
        let mirror_after_them = half_ka_v2_hm::make_feature_bucket(them, &self.root_pos).1;
        dirty.requires_refresh[us as usize] |= mirror_before_us != mirror_after_us;
        dirty.requires_refresh[them as usize] |= mirror_before_them != mirror_after_them;

        self.acc_stack.push();
        self.acc_stack.set_psq_diff(dirty);
        self.acc_stack.set_threat_diff(dts);
    }

    pub fn push_acc(&mut self) {
        self.acc_stack.push();
    }

    pub fn pop_acc(&mut self) {
        self.acc_stack.pop();
    }

    pub fn reset_acc(&mut self) {
        self.acc_stack.reset();
    }

    pub fn check_time(&mut self) {
        self.calls_cnt -= 1;
        if self.calls_cnt > 0 {
            return;
        }
        self.calls_cnt = if self.limits.nodes > 0 {
            512.min((self.limits.nodes / 1024) as i32)
        } else {
            512
        };

        let elapsed = self.tm.elapsed();

        if self.ponder.load(Ordering::Relaxed) {
            return;
        }

        if (self.limits.use_time_management()
            && (elapsed > self.tm.maximum() || self.stop_on_ponderhit))
            || (self.limits.movetime > 0 && elapsed >= self.limits.movetime)
            || (self.limits.nodes > 0 && self.node_count() >= self.limits.nodes)
        {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    pub fn reset_ss(&mut self) {
        self.ss_static_evals.fill(VALUE_NONE);
        self.ss_tt_pvs.fill(false);
        self.ss_excluded_moves.fill(Move::NONE);
        self.ss_in_check.fill(false);
        self.ss_current_moves.fill(Move::NONE);
        self.ss_move_counts.fill(0);
        self.ss_cutoff_cnts.fill(0);
        self.ss_reductions.fill(0);
        self.ss_stat_scores.fill(0);
        self.ss_follow_pvs.fill(false);
        self.ss_cont_hist_indices.fill(ContHistIndex::SENTINEL);
        self.ss_tt_hits.fill(false);
        self.ss_prior_nmp_fail_high.fill(0);
    }

    #[inline]
    pub const fn ss_idx(&self, ply: i32) -> usize {
        (ply + SS_OFFSET as i32) as usize
    }
}

pub const fn value_to_tt(v: Value, ply: i32) -> Value {
    use crate::types::{is_loss, is_win};
    if is_win(v) {
        v + ply
    } else if is_loss(v) {
        v - ply
    } else {
        v
    }
}

pub const fn value_from_tt(v: Value, ply: i32, r60c: i32) -> Value {
    use crate::types::{VALUE_MATE, is_loss, is_win};
    if !is_valid(v) {
        return VALUE_NONE;
    }
    if is_win(v) {
        return if VALUE_MATE - v > 120 - r60c {
            VALUE_MATE_IN_MAX_PLY - 1
        } else {
            v - ply
        };
    }
    if is_loss(v) {
        return if VALUE_MATE + v > 120 - r60c {
            VALUE_MATED_IN_MAX_PLY + 1
        } else {
            v + ply
        };
    }
    v
}

pub fn to_corrected_static_eval(v: Value, cv: i32) -> Value {
    (v + cv / 131_072).clamp(VALUE_MATED_IN_MAX_PLY + 1, VALUE_MATE_IN_MAX_PLY - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_weighted_averages_preserve_negative_reference_rounding() {
        let mut root = RootMove::new(Move::NONE);
        root.effort = 5;
        root.average_score = -100;
        root.mean_squared_score = -10_000;
        root.update_average(-101, 3);
        // Independently evaluated with b562d6ae's u64-weighted C++ expressions.
        // Both divisions have negative non-integral mathematical results.
        assert_eq!(root.effort, 8);
        assert_eq!(root.average_score, -101);
        assert_eq!(root.mean_squared_score, -10_076);
    }

    #[test]
    fn effort_weighted_average_caps_score_and_squared_score_weights_separately() {
        let mut root = RootMove::new(Move::NONE);
        root.effort = 1;
        root.average_score = 100;
        root.mean_squared_score = 10_000;
        root.update_average(200, 100);
        assert_eq!(root.average_score, 175);
        assert_eq!(root.mean_squared_score, 25_000);
    }

    #[test]
    fn bounded_mate_loss_is_not_an_exact_root_loss() {
        let mut root = RootMove::new(Move::NONE);
        root.score = crate::types::mated_in(4);
        assert!(root.is_exact_loss());
        root.score_lowerbound = true;
        assert!(!root.is_exact_loss());
        root.unset_inexact();
        assert!(root.is_exact_loss());
    }
}
