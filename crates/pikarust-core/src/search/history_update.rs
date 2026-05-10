//! History update functions matching Pikafish's `update_all_stats`,
//! `update_quiet_histories`, `update_continuation_histories`, `correction_value`,
//! and `update_correction_history`.

use crate::types::{Color, Move, Piece, Square};

use super::history::{ContHistIndex, LOW_PLY_HISTORY_SIZE};
use super::search::Worker;

/// Conthist bonus weights: (offset, weight) pairs matching Pikafish.
const CONTHIST_BONUSES: [(usize, i32); 6] =
    [(1, 1076), (2, 639), (3, 293), (4, 523), (5, 129), (6, 445)];

/// Multipliers for positive history consistency (index 0 unused, 1..=6 used).
const CMHC_MULTIPLIERS: [i32; 7] = [96, 100, 100, 100, 115, 118, 129];

impl Worker {
    /// Compute the correction value from all correction history tables.
    /// Matches C++ `correction_value(worker, pos, ss)`.
    pub fn correction_value(&self, ply: i32) -> i32 {
        let ss = self.ss_idx(ply);
        let us = self.root_pos.side_to_move();

        let pawn_key = self.root_pos.pawn_key();
        let minor_key = self.root_pos.minor_piece_key();
        let wnp_key = self.root_pos.non_pawn_key(Color::White);
        let bnp_key = self.root_pos.non_pawn_key(Color::Black);

        let pcv = i32::from(self.correction_history.entry(pawn_key)[us.index()].pawn);
        let micv = i32::from(self.correction_history.entry(minor_key)[us.index()].minor);
        let wnpcv = i32::from(self.correction_history.entry(wnp_key)[us.index()].non_pawn_white);
        let bnpcv = i32::from(self.correction_history.entry(bnp_key)[us.index()].non_pawn_black);

        // Continuation correction: (ss-1)->currentMove
        let prev_move = if ss >= 1 {
            self.ss_current_moves[ss - 1]
        } else {
            Move::NONE
        };

        let cntcv = if prev_move.is_ok() {
            let to = prev_move.to_sq();
            let pc = self.root_pos.piece_on(to);

            // (ss-2)->continuationCorrectionHistory[pc][to]
            let idx2 = self.cont_corr_index(ss, 2);
            let v2 = i32::from(
                self.continuation_correction_history
                    .get(idx2.pc, idx2.sq)
                    .get(pc, to),
            );

            // (ss-4)->continuationCorrectionHistory[pc][to]
            let idx4 = self.cont_corr_index(ss, 4);
            let v4 = i32::from(
                self.continuation_correction_history
                    .get(idx4.pc, idx4.sq)
                    .get(pc, to),
            );

            v2 + v4
        } else {
            8
        };

        4547 * pcv + 3804 * micv + 8213 * (wnpcv + bnpcv) + 8982 * cntcv
    }

    /// Update all correction history tables.
    /// Matches C++ `update_correction_history(pos, ss, worker, bonus)`.
    pub fn update_correction_history(&mut self, ply: i32, bonus: i32) {
        const NON_PAWN_WEIGHT: i32 = 125;

        let ss = self.ss_idx(ply);
        let us = self.root_pos.side_to_move();

        let pawn_key = self.root_pos.pawn_key();
        let minor_key = self.root_pos.minor_piece_key();
        let wnp_key = self.root_pos.non_pawn_key(Color::White);
        let bnp_key = self.root_pos.non_pawn_key(Color::Black);

        self.correction_history.entry_mut(pawn_key)[us.index()].update_pawn(bonus);
        self.correction_history.entry_mut(minor_key)[us.index()].update_minor(bonus * 145 / 128);
        self.correction_history.entry_mut(wnp_key)[us.index()]
            .update_non_pawn_white(bonus * NON_PAWN_WEIGHT / 128);
        self.correction_history.entry_mut(bnp_key)[us.index()]
            .update_non_pawn_black(bonus * NON_PAWN_WEIGHT / 128);

        // Continuation correction history update
        let prev_move = if ss >= 1 {
            self.ss_current_moves[ss - 1]
        } else {
            Move::NONE
        };

        if prev_move.is_ok() {
            let to = prev_move.to_sq();
            let pc = self.root_pos.piece_on(to);

            // (ss-2)->continuationCorrectionHistory
            let idx2 = self.cont_corr_index(ss, 2);
            self.continuation_correction_history
                .get_mut(idx2.pc, idx2.sq)
                .update(pc, to, bonus * 131 / 128);

            // (ss-4)->continuationCorrectionHistory
            let idx4 = self.cont_corr_index(ss, 4);
            self.continuation_correction_history
                .get_mut(idx4.pc, idx4.sq)
                .update(pc, to, bonus * 63 / 128);
        }
    }

    /// Update all stats when a best move is found.
    /// Matches C++ `update_all_stats(pos, ss, worker, bestMove, prevSq, quiets, captures, depth, ttMove)`.
    #[allow(clippy::too_many_arguments)]
    pub fn update_all_stats(
        &mut self,
        ply: i32,
        best_move: Move,
        prev_sq: Option<Square>,
        quiets_searched: &[Move],
        captures_searched: &[Move],
        depth: i32,
        tt_move: Move,
    ) {
        let ss = self.ss_idx(ply);
        let moved_piece = self.root_pos.moved_piece(best_move);

        let stat_score_prev = if ss >= 1 {
            self.ss_stat_scores[ss - 1]
        } else {
            0
        };
        let bonus = (162 * depth - 87).min(1602)
            + 336 * i32::from(best_move == tt_move)
            + stat_score_prev / 32;
        let malus = (870 * depth - 148).min(2000);

        if self.root_pos.is_capture(best_move) {
            // Increase stats for the best move in case it was a capture move
            let captured_pt = self.root_pos.piece_on(best_move.to_sq()).piece_type();
            self.capture_history.update(
                moved_piece,
                best_move.to_sq(),
                captured_pt,
                bonus * 1455 / 1024,
            );
        } else {
            self.update_quiet_histories(ply, best_move, bonus * 899 / 1024);

            let mut actual_malus = malus * 1100 / 1024;
            // Decrease stats for all non-best quiet moves
            for &qm in quiets_searched {
                actual_malus = actual_malus * 950 / 1024;
                self.update_quiet_histories(ply, qm, -actual_malus);
            }
        }

        // Extra penalty for a quiet early move that was not a TT move in
        // previous ply when it gets refuted.
        if let Some(psq) = prev_sq {
            let prev_ply_ss = ss.saturating_sub(1);
            let prev_move_count = self.ss_move_counts[prev_ply_ss];
            let prev_tt_hit = self.ss_tt_hits[prev_ply_ss];
            if prev_move_count == 1 + i32::from(prev_tt_hit)
                && self.root_pos.captured_piece() == Piece::NONE
            {
                let pc_on_prev = self.root_pos.piece_on(psq);
                self.update_continuation_histories(ply - 1, pc_on_prev, psq, -malus * 617 / 1024);
            }
        }

        // Decrease stats for all non-best capture moves
        for &cm in captures_searched {
            let cap_moved = self.root_pos.moved_piece(cm);
            let cap_pt = self.root_pos.piece_on(cm.to_sq()).piece_type();
            self.capture_history
                .update(cap_moved, cm.to_sq(), cap_pt, -malus * 1440 / 1024);
        }
    }

    /// Update quiet move histories (main, `low_ply`, continuation, pawn).
    /// Matches C++ `update_quiet_histories(pos, ss, worker, move, bonus)`.
    pub fn update_quiet_histories(&mut self, ply: i32, m: Move, bonus: i32) {
        let us = self.root_pos.side_to_move();
        let moved_piece = self.root_pos.moved_piece(m);
        let to = m.to_sq();

        // mainHistory
        self.main_history.update(us, m, bonus);

        // lowPlyHistory
        if (ply as usize) < LOW_PLY_HISTORY_SIZE {
            self.low_ply_history
                .update(ply as usize, m, bonus * 693 / 1024);
        }

        // continuation histories
        self.update_continuation_histories(ply, moved_piece, to, bonus * 972 / 1024);

        // pawn history
        let pawn_key = self.root_pos.pawn_key();
        let pawn_bonus = if bonus > 0 {
            bonus * 913 / 1024
        } else {
            bonus * 553 / 1024
        };
        self.pawn_history
            .entry_mut(pawn_key)
            .update(moved_piece, to, pawn_bonus);
    }

    /// Update continuation histories for the move pairs formed by the current
    /// move and moves played in previous plies.
    /// Matches C++ `update_continuation_histories(ss, pc, to, bonus)`.
    pub fn update_continuation_histories(&mut self, ply: i32, pc: Piece, to: Square, bonus: i32) {
        let ss = self.ss_idx(ply);
        let in_check = self.ss_in_check[ss];
        let mut positive_count: usize = 0;

        for &(i, weight) in &CONTHIST_BONUSES {
            // Only update the first 2 continuation histories if we are in check
            if in_check && i > 2 {
                break;
            }

            if ss < i {
                continue;
            }

            let prev_ss = ss - i;
            let prev_move = self.ss_current_moves[prev_ss];
            if !prev_move.is_ok() {
                continue;
            }

            // Get the continuation history entry for (ss - i)
            let idx = self.ss_cont_hist_indices[prev_ss];
            let hist_entry_val = self
                .continuation_history
                .get(idx.in_check, idx.capture, idx.pc, idx.sq)
                .get(pc, to);

            if hist_entry_val > 0 {
                positive_count += 1;
            }

            let multiplier = CMHC_MULTIPLIERS[positive_count.min(6)];
            let adjusted_bonus = (bonus * weight * multiplier / 131_072) + 83 * i32::from(i < 2);

            // Now update the entry
            self.continuation_history
                .get_mut(idx.in_check, idx.capture, idx.pc, idx.sq)
                .update(pc, to, adjusted_bonus);
        }
    }

    /// Get combined continuation history value for a move (contHist\[0\] + contHist\[1\]).
    /// Used in Step 13 quiet pruning.
    pub fn get_cont_hist_value(&self, ply: i32, pc: Piece, to: Square) -> i32 {
        let ss = self.ss_idx(ply);
        let mut val = 0i32;
        for offset in 1..=2 {
            if ss >= offset {
                let idx = self.ss_cont_hist_indices[ss - offset];
                val += i32::from(
                    self.continuation_history
                        .get(idx.in_check, idx.capture, idx.pc, idx.sq)
                        .get(pc, to),
                );
            }
        }
        val
    }

    /// Helper: get the `ContHistIndex` for `(ss - offset)`, used for
    /// continuation correction history lookups.
    #[inline]
    fn cont_corr_index(&self, ss: usize, offset: usize) -> ContHistIndex {
        if ss >= offset {
            self.ss_cont_hist_indices[ss - offset]
        } else {
            ContHistIndex::SENTINEL
        }
    }

    /// Build the contHist array for `MovePicker` from the search stack.
    /// Returns up to 6 references to `PieceToHistory` tables.
    /// Matches C++ `contHist[] = {(ss-1)->continuationHistory, ..., (ss-6)->continuationHistory}`.
    pub fn build_cont_hist_for_movepicker(
        &self,
        ply: i32,
    ) -> ([&super::history::PieceToHistory; 6], usize) {
        let ss = self.ss_idx(ply);
        // Use a sentinel reference for unused slots (never read past `len`).
        let sentinel = self.continuation_history.get(
            false,
            false,
            crate::types::Piece::NONE,
            crate::types::Square::SQ_A0,
        );
        let mut buf = [sentinel; 6];
        let mut len = 0;
        for offset in 1..=6 {
            if ss >= offset {
                let idx = self.ss_cont_hist_indices[ss - offset];
                buf[len] = self
                    .continuation_history
                    .get(idx.in_check, idx.capture, idx.pc, idx.sq);
                len += 1;
            }
        }
        (buf, len)
    }

    /// Set the continuation history index for the current ply after making a move.
    /// This is the Rust equivalent of C++:
    /// `ss->continuationHistory = &continuationHistory[inCheck][capture][pc][to]`
    pub fn set_cont_hist_index(
        &mut self,
        ply: i32,
        in_check: bool,
        capture: bool,
        pc: Piece,
        to: Square,
    ) {
        let ss = self.ss_idx(ply);
        self.ss_cont_hist_indices[ss] = ContHistIndex {
            in_check,
            capture,
            pc,
            sq: to,
        };
    }

    /// Set the continuation history index to sentinel (for null moves).
    pub fn set_cont_hist_index_sentinel(&mut self, ply: i32) {
        let ss = self.ss_idx(ply);
        self.ss_cont_hist_indices[ss] = ContHistIndex::SENTINEL;
    }
}
