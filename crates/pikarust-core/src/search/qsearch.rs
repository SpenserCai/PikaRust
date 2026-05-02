use crate::position::{GenType, generate};
use crate::types::{
    Bound, DEPTH_QS, DEPTH_UNSEARCHED, MAX_PLY, Move, PIECE_VALUE, VALUE_DRAW, VALUE_INFINITE,
    VALUE_NONE, Value, is_decisive, is_loss, is_valid, mated_in,
};

use super::movepick::MovePicker;
use super::search::{Worker, to_corrected_static_eval, value_from_tt, value_to_tt};

impl Worker {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn qsearch(&mut self, ply: i32, mut alpha: Value, beta: Value, pv_node: bool) -> Value {
        let ss = self.ss_idx(ply);

        let mut best_move = Move::NONE;
        let in_check = self.root_pos.checkers().is_not_empty();
        self.ss_in_check[ss] = in_check;
        let mut move_count = 0;

        if pv_node && self.sel_depth < ply + 1 {
            self.sel_depth = ply + 1;
        }

        // Check for repetition or max ply
        if let Some(result) = self.root_pos.rule_judge(ply) {
            return result;
        }

        if ply >= MAX_PLY {
            return if in_check {
                VALUE_DRAW
            } else {
                self.evaluate_pos()
            };
        }

        // TT lookup
        let pos_key = self.root_pos.key();
        let probe = self.tt.probe(pos_key);
        let tt_hit = probe.found;
        let mut tt_data = probe.data;
        let tt_writer = probe.writer;

        if !tt_hit {
            tt_data.tt_move = Move::NONE;
        }
        if tt_hit {
            tt_data.value = value_from_tt(tt_data.value, ply, self.root_pos.rule60_count());
        } else {
            tt_data.value = VALUE_NONE;
        }

        let pv_hit = tt_hit && tt_data.is_pv;

        // TT cutoff for non-PV
        if !pv_node && tt_data.depth >= DEPTH_QS && is_valid(tt_data.value) {
            let bound_ok = if tt_data.value >= beta {
                tt_data.bound as u8 & Bound::Lower as u8 != 0
            } else {
                tt_data.bound as u8 & Bound::Upper as u8 != 0
            };
            if bound_ok {
                return tt_data.value;
            }
        }

        // Static evaluation
        let mut unadjusted_static_eval = VALUE_NONE;
        let mut best_value;

        let futility_base = if in_check {
            best_value = -VALUE_INFINITE;
            -VALUE_INFINITE
        } else {
            let correction_val = self.correction_value(ply);
            if tt_hit {
                unadjusted_static_eval = tt_data.eval;
                if !is_valid(unadjusted_static_eval) {
                    unadjusted_static_eval = self.evaluate_pos();
                }
            } else {
                unadjusted_static_eval = self.evaluate_pos();
            }
            best_value = to_corrected_static_eval(unadjusted_static_eval, correction_val);
            self.ss_static_evals[ss] = best_value;

            if tt_hit && is_valid(tt_data.value) && !is_decisive(tt_data.value) {
                let bound_ok = if tt_data.value > best_value {
                    tt_data.bound as u8 & Bound::Lower as u8 != 0
                } else {
                    tt_data.bound as u8 & Bound::Upper as u8 != 0
                };
                if bound_ok {
                    best_value = tt_data.value;
                }
            }

            // Stand pat
            if best_value >= beta {
                if !is_decisive(best_value) {
                    best_value = (best_value + beta) / 2;
                }
                if !tt_hit {
                    tt_writer.write(
                        pos_key,
                        VALUE_NONE,
                        false,
                        Bound::Lower,
                        DEPTH_UNSEARCHED,
                        Move::NONE,
                        unadjusted_static_eval,
                        self.tt.generation(),
                    );
                }
                return best_value;
            }

            if best_value > alpha {
                alpha = best_value;
            }

            self.ss_static_evals[ss] + 220
        };

        let prev_sq = if ss > 0 && self.ss_current_moves[ss - 1].is_ok() {
            Some(self.ss_current_moves[ss - 1].to_sq())
        } else {
            None
        };

        // Move generation — build contHist from search stack (only ss-1 for qsearch)
        let cont_hist_refs = self.build_cont_hist_for_movepicker(ply);
        // qsearch only uses the first entry, but pass all available
        let cont_hist_slice: Vec<&super::history::PieceToHistory> = cont_hist_refs;

        let mut mp = MovePicker::new_main(
            &self.root_pos,
            tt_data.tt_move,
            DEPTH_QS,
            &self.main_history,
            &self.low_ply_history,
            &self.capture_history,
            &cont_hist_slice,
            &self.pawn_history,
            ply,
        );

        loop {
            let m = mp.next_move(&self.root_pos);
            if m == Move::NONE {
                break;
            }

            if !self.root_pos.is_legal(m) {
                continue;
            }

            let gives_check = self.root_pos.gives_check(m);
            let capture = self.root_pos.is_capture(m);

            move_count += 1;

            // Pruning
            if !is_loss(best_value) {
                if !gives_check
                    && prev_sq.is_some_and(|psq| m.to_sq() != psq)
                    && !is_loss(futility_base)
                {
                    if move_count > 2 {
                        continue;
                    }

                    let futility_value =
                        futility_base + PIECE_VALUE[self.root_pos.piece_on(m.to_sq())];
                    if futility_value <= alpha {
                        best_value = best_value.max(futility_value);
                        continue;
                    }

                    if !self.root_pos.see_ge(m, alpha - futility_base) {
                        best_value = best_value.max(alpha.min(futility_base));
                        continue;
                    }
                }

                if !capture {
                    continue;
                }

                if !self.root_pos.see_ge(m, -106) {
                    continue;
                }
            }

            // Make and search
            self.ss_current_moves[ss] = m;
            self.push_acc_for_move(m);
            self.root_pos.do_move(m, gives_check);
            self.root_pos.debug_check_consistency("after_do_move_qs");
            self.inc_nodes();

            let value = -self.qsearch(ply + 1, -beta, -alpha, pv_node);

            self.root_pos.undo_move(m);
            self.pop_acc();
            self.root_pos.debug_check_consistency("after_undo_move_qs");

            if value > best_value {
                best_value = value;
                if value > alpha {
                    best_move = m;
                    if value < beta {
                        alpha = value;
                    } else {
                        break;
                    }
                }
            }
        }

        // Check for mate
        if move_count == 0 && in_check {
            let has_quiet = {
                let ml = generate(&self.root_pos, GenType::Quiets);
                let mut found = false;
                for i in 0..ml.len() {
                    if self.root_pos.is_legal(ml.get(i)) {
                        found = true;
                        break;
                    }
                }
                found
            };
            if !has_quiet {
                return mated_in(ply);
            }
        }

        if !is_decisive(best_value) && best_value > beta {
            best_value = (best_value + beta) / 2;
        }

        tt_writer.write(
            pos_key,
            value_to_tt(best_value, ply),
            pv_hit,
            if best_value >= beta {
                Bound::Lower
            } else {
                Bound::Upper
            },
            DEPTH_QS,
            best_move,
            unadjusted_static_eval,
            self.tt.generation(),
        );

        best_value
    }
}
