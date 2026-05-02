use std::sync::atomic::Ordering;

use crate::types::{
    Bound, DEPTH_UNSEARCHED, Depth, MAX_PLY, Move, PIECE_VALUE, VALUE_DRAW, VALUE_INFINITE,
    VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY, VALUE_NONE, VALUE_ZERO, Value, is_decisive,
    is_loss, is_valid, is_win, mate_in, mated_in,
};

use super::history::LOW_PLY_HISTORY_SIZE;
use super::movepick::MovePicker;
use super::search::{
    SEARCHED_LIST_CAPACITY, Worker, to_corrected_static_eval, value_from_tt, value_to_tt,
};

impl Worker {
    #[allow(clippy::too_many_lines)]
    pub fn iterative_deepening(&mut self) -> Option<Move> {
        let is_main = self.is_main_thread();
        let us = self.root_pos.side_to_move();
        let mut best_value;
        let mut iter_idx: usize = 0;
        let mut search_again_counter = 0;

        self.reset_ss();

        if is_main {
            if self.best_previous_score == VALUE_INFINITE {
                self.iter_value.fill(VALUE_ZERO);
            } else {
                self.iter_value.fill(self.best_previous_score);
            }
            self.tm.init(
                &self.limits,
                us,
                i32::from(self.root_pos.game_ply()),
                50,
                self.limits.ponder_mode,
            );
        }

        self.low_ply_history.fill(99);
        self.main_history.scale(768, 1024);

        while self.root_depth + 1 < MAX_PLY
            && !self.stop.load(Ordering::Relaxed)
            && !(self.limits.depth > 0 && is_main && self.root_depth >= self.limits.depth)
        {
            self.root_depth += 1;

            for rm in &mut self.root_moves {
                rm.previous_score = rm.score;
            }

            if !self.increase_depth.load(Ordering::Relaxed) {
                search_again_counter += 1;
            }

            self.pv_last = self.root_moves.len();
            self.sel_depth = 0;

            let mut delta = 10
                + (self.thread_idx % 8) as i32
                + (self.root_moves[0].mean_squared_score.unsigned_abs() / 39605) as i32;
            let avg = self.root_moves[0].average_score;
            let mut alpha = (avg - delta).max(-VALUE_INFINITE);
            let mut beta = (avg + delta).min(VALUE_INFINITE);

            self.optimism[us.index()] = 92 * avg / (avg.abs() + 95);
            self.optimism[(!us).index()] = -self.optimism[us.index()];

            let mut failed_high_cnt = 0;
            loop {
                let adjusted_depth =
                    1.max(self.root_depth - failed_high_cnt - 3 * (search_again_counter + 1) / 4);
                self.root_delta = beta - alpha;
                best_value = self.ab_search::<true>(0, alpha, beta, adjusted_depth, false);

                self.root_moves[self.pv_idx..self.pv_last].sort_by(|a, b| {
                    b.score
                        .cmp(&a.score)
                        .then(b.previous_score.cmp(&a.previous_score))
                });

                if self.stop.load(Ordering::Relaxed) {
                    break;
                }

                if best_value <= alpha {
                    beta = alpha;
                    alpha = (best_value - delta).max(-VALUE_INFINITE);
                    failed_high_cnt = 0;
                    if is_main {
                        self.stop_on_ponderhit = false;
                    }
                } else if best_value >= beta {
                    alpha = (beta - delta).max(alpha);
                    beta = (best_value + delta).min(VALUE_INFINITE);
                    failed_high_cnt += 1;
                } else {
                    break;
                }

                delta += delta / 3;
            }

            if !self.stop.load(Ordering::Relaxed) {
                self.completed_depth = self.root_depth;

                if self.last_iteration_pv.is_empty()
                    || self.root_moves[0].pv[0] != self.last_iteration_pv[0]
                {
                    // best move changed
                }

                self.last_iteration_pv = self.root_moves[0].pv.clone();
            }

            if self.limits.mate > 0
                && !self.stop.load(Ordering::Relaxed)
                && ((self.root_moves[0].score >= VALUE_MATE_IN_MAX_PLY
                    && crate::types::VALUE_MATE - self.root_moves[0].score <= 2 * self.limits.mate)
                    || (self.root_moves[0].score <= VALUE_MATED_IN_MAX_PLY
                        && crate::types::VALUE_MATE + self.root_moves[0].score
                            <= 2 * self.limits.mate))
            {
                self.stop.store(true, Ordering::Relaxed);
            }

            if !is_main {
                continue;
            }

            if self.limits.use_time_management()
                && !self.stop.load(Ordering::Relaxed)
                && !self.stop_on_ponderhit
            {
                let elapsed = self.tm.elapsed();
                if elapsed > self.tm.maximum() {
                    if self.limits.ponder_mode {
                        self.stop_on_ponderhit = true;
                    } else {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                }
            }

            self.iter_value[iter_idx] = best_value;
            iter_idx = (iter_idx + 1) & 3;
        }

        if is_main {
            self.best_previous_score = self.root_moves[0].score;
            self.best_previous_avg_score = self.root_moves[0].average_score;
        }

        if self.root_moves.is_empty() {
            return None;
        }

        Some(self.root_moves[0].pv[0])
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    pub fn ab_search<const ROOT: bool>(
        &mut self,
        ply: i32,
        mut alpha: Value,
        mut beta: Value,
        mut depth: Depth,
        cut_node: bool,
    ) -> Value {
        let pv_node = ROOT || alpha + 1 != beta;
        let all_node = !pv_node && !cut_node;
        let ss = self.ss_idx(ply);

        if depth <= 0 {
            return self.qsearch(ply, alpha, beta, pv_node);
        }

        depth = depth.min(MAX_PLY - 1);

        let in_check = self.root_pos.checkers().is_not_empty();
        let us = self.root_pos.side_to_move();

        if self.is_main_thread() {
            self.check_time();
        }

        if pv_node && self.sel_depth < ply + 1 {
            self.sel_depth = ply + 1;
        }

        if !ROOT {
            if let Some(result) = self.root_pos.rule_judge(ply) {
                return if result == VALUE_DRAW {
                    self.value_draw()
                } else {
                    result
                };
            }

            if self.stop.load(Ordering::Relaxed) || ply >= MAX_PLY {
                return if ply >= MAX_PLY && !in_check {
                    self.evaluate_pos()
                } else {
                    self.value_draw()
                };
            }

            alpha = alpha.max(mated_in(ply));
            beta = beta.min(mate_in(ply + 1));
            if alpha >= beta {
                return alpha;
            }
        }

        let pos_key = self.root_pos.key();
        let probe = self.tt.probe(pos_key);
        let tt_hit = probe.found;
        let mut tt_data = probe.data;
        let tt_writer = probe.writer;

        if ROOT {
            tt_data.tt_move = self.root_moves[self.pv_idx].pv[0];
        } else if !tt_hit {
            tt_data.tt_move = Move::NONE;
        }

        if tt_hit {
            tt_data.value = value_from_tt(tt_data.value, ply, self.root_pos.rule60_count());
        } else {
            tt_data.value = VALUE_NONE;
        }

        let excluded_move = self.ss_excluded_moves[ss];
        let tt_pv = if excluded_move.is_ok() {
            self.ss_tt_pvs[ss]
        } else {
            pv_node || (tt_hit && tt_data.is_pv)
        };
        self.ss_tt_pvs[ss] = tt_pv;

        let tt_capture = tt_data.tt_move.is_ok() && self.root_pos.is_capture(tt_data.tt_move);

        // Static evaluation
        let mut unadjusted_static_eval = VALUE_NONE;
        let eval;

        if in_check {
            eval = if ss >= 2 {
                self.ss_static_evals[ss - 2]
            } else {
                VALUE_NONE
            };
            self.ss_static_evals[ss] = eval;
        } else if excluded_move.is_ok() {
            unadjusted_static_eval = self.ss_static_evals[ss];
            eval = unadjusted_static_eval;
        } else if tt_hit {
            unadjusted_static_eval = tt_data.eval;
            if !is_valid(unadjusted_static_eval) {
                unadjusted_static_eval = self.evaluate_pos();
            }
            eval = to_corrected_static_eval(unadjusted_static_eval, 0);
            self.ss_static_evals[ss] = eval;
        } else {
            unadjusted_static_eval = self.evaluate_pos();
            eval = to_corrected_static_eval(unadjusted_static_eval, 0);
            self.ss_static_evals[ss] = eval;

            tt_writer.write(
                pos_key,
                VALUE_NONE,
                tt_pv,
                Bound::None,
                DEPTH_UNSEARCHED,
                Move::NONE,
                unadjusted_static_eval,
                self.tt.generation(),
            );
        }

        let improving = ss >= 2 && eval > self.ss_static_evals[ss - 2];

        // TT cutoff for non-PV nodes
        if !pv_node
            && !excluded_move.is_ok()
            && tt_data.depth > depth - i32::from(tt_data.value <= beta)
            && is_valid(tt_data.value)
        {
            let bound_ok = if tt_data.value >= beta {
                tt_data.bound as u8 & Bound::Lower as u8 != 0
            } else {
                tt_data.bound as u8 & Bound::Upper as u8 != 0
            };
            if bound_ok
                && ((cut_node == (tt_data.value >= beta)) || depth > 5)
                && self.root_pos.rule60_count() < 116
            {
                return tt_data.value;
            }
        }

        if !in_check {
            // Razoring
            if !pv_node && eval < alpha - 1370 - 244 * depth * depth {
                return self.qsearch(ply, alpha, beta, false);
            }

            // Futility pruning
            if !tt_pv
                && depth < 15
                && eval >= beta
                && (!tt_data.tt_move.is_ok() || tt_capture)
                && !is_loss(beta)
                && !is_win(eval)
            {
                let fm = 129 - 33 * i32::from(!tt_hit);
                let margin = fm * depth - (2512 * i32::from(improving)) * fm / 1024;
                if eval - margin >= beta {
                    return (2 * beta + eval) / 3;
                }
            }

            // Null move search
            if cut_node
                && eval >= beta - 8 * depth - 50 * i32::from(improving) + 187
                && !excluded_move.is_ok()
                && self.root_pos.major_material(us) > 0
                && ply >= self.nmp_min_ply
                && !is_loss(beta)
            {
                let r = 8 + depth / 3;
                self.ss_current_moves[ss] = Move::NULL;
                self.root_pos.do_null_move();
                self.inc_nodes();
                let null_value =
                    -self.ab_search::<false>(ply + 1, -beta, -beta + 1, depth - r, false);
                self.root_pos.undo_null_move();

                if null_value >= beta && !is_win(null_value) {
                    if self.nmp_min_ply > 0 || depth < 15 {
                        return null_value;
                    }
                    self.nmp_min_ply = ply + 3 * (depth - r) / 4;
                    let v = self.ab_search::<false>(ply, beta - 1, beta, depth - r, false);
                    self.nmp_min_ply = 0;
                    if v >= beta {
                        return null_value;
                    }
                }
            }

            // IIR
            if !all_node && depth >= 6 && !tt_data.tt_move.is_ok() {
                depth -= 1;
            }
        }

        // Moves loop
        let cont_hist_sentinel = super::history::PieceToHistory::new();
        let cont_hist: [&super::history::PieceToHistory; 1] = [&cont_hist_sentinel];

        let mut mp = MovePicker::new_main(
            &self.root_pos,
            tt_data.tt_move,
            depth,
            &self.main_history,
            &self.low_ply_history,
            &self.capture_history,
            &cont_hist,
            &self.pawn_history,
            ply,
        );

        let mut best_value = -VALUE_INFINITE;
        let mut best_move = Move::NONE;
        let mut move_count = 0i32;
        let mut quiets_searched = Vec::with_capacity(SEARCHED_LIST_CAPACITY);

        self.ss_move_counts[ss] = 0;
        if ss + 2 < self.ss_cutoff_cnts.len() {
            self.ss_cutoff_cnts[ss + 2] = 0;
        }

        loop {
            let m = mp.next_move(&self.root_pos);
            if m == Move::NONE {
                break;
            }

            if m == excluded_move {
                continue;
            }

            if !self.root_pos.is_legal(m) {
                continue;
            }

            if ROOT
                && !self.root_moves[self.pv_idx..self.pv_last]
                    .iter()
                    .any(|rm| rm.pv[0] == m)
            {
                continue;
            }

            move_count += 1;
            self.ss_move_counts[ss] = move_count;

            let capture = self.root_pos.is_capture(m);
            let gives_check = self.root_pos.gives_check(m);
            let _moved_piece = self.root_pos.moved_piece(m);

            let mut new_depth = depth - 1;
            let mut extension: i32 = 0;

            let delta = beta - alpha;
            let r = self.reduction(improving, depth, move_count, delta);

            // LMP
            if !ROOT
                && self.root_pos.major_material(us) > 0
                && !is_loss(best_value)
                && move_count >= (3 + depth * depth) / (2 - i32::from(improving))
                && !capture
                && !gives_check
            {
                continue;
            }

            // Singular extension
            if !ROOT
                && m == tt_data.tt_move
                && !excluded_move.is_ok()
                && depth >= 5 + i32::from(tt_pv)
                && is_valid(tt_data.value)
                && !is_decisive(tt_data.value)
                && (tt_data.bound as u8 & Bound::Lower as u8) != 0
                && tt_data.depth >= depth - 3
            {
                let sb = tt_data.value - (44 + 72 * i32::from(tt_pv && !pv_node)) * depth / 69;
                let sd = new_depth / 2;

                self.ss_excluded_moves[ss] = m;
                let value = self.ab_search::<false>(ply, sb - 1, sb, sd, cut_node);
                self.ss_excluded_moves[ss] = Move::NONE;

                if value < sb {
                    extension = 1 + i32::from(value < sb + 4) + i32::from(value < sb - 106);
                    depth += 1;
                } else if value >= beta && !is_decisive(value) {
                    return value;
                } else if tt_data.value >= beta {
                    extension = -3;
                } else if cut_node {
                    extension = -2;
                }
            }

            // Make move
            self.ss_current_moves[ss] = m;
            self.ss_in_check[ss] = gives_check;
            self.root_pos.do_move(m, gives_check);
            self.inc_nodes();

            new_depth += extension;
            let node_count = if ROOT { self.node_count() } else { 0 };

            let mut value;

            // LMR
            if depth >= 2 && move_count > 1 {
                let mut r_adj = r;

                if tt_pv {
                    r_adj -= 2363 + i32::from(pv_node) * 963;
                }
                r_adj += 855;
                r_adj -= move_count * 64;

                if cut_node {
                    r_adj += 3251 + 1048 * i32::from(!tt_data.tt_move.is_ok());
                }
                if tt_capture {
                    r_adj += 1571;
                }
                if m == tt_data.tt_move {
                    r_adj -= 2953;
                }

                if capture {
                    let stat = 953 * PIECE_VALUE[self.root_pos.captured_piece()] / 128;
                    r_adj -= stat * 946 / 8192;
                } else {
                    let stat = 2 * i32::from(self.main_history.get(us, m));
                    r_adj -= stat * 946 / 8192;
                }

                if all_node {
                    r_adj += r_adj * 256 / (256 * depth + 256);
                }

                let d = 1.max((new_depth - r_adj / 1024).min(new_depth + 2)) + i32::from(pv_node);

                self.ss_reductions[ss] = new_depth - d;
                value = -self.ab_search::<false>(ply + 1, -(alpha + 1), -alpha, d, true);
                self.ss_reductions[ss] = 0;

                if value > alpha {
                    let do_deeper = d < new_depth && value > best_value + 60;
                    let do_shallower = value < best_value + 9;
                    new_depth += i32::from(do_deeper) - i32::from(do_shallower);

                    if new_depth > d {
                        value = -self.ab_search::<false>(
                            ply + 1,
                            -(alpha + 1),
                            -alpha,
                            new_depth,
                            !cut_node,
                        );
                    }
                }
            } else if !pv_node || move_count > 1 {
                let r_extra = if tt_data.tt_move.is_ok() { 0 } else { 979 };
                value = -self.ab_search::<false>(
                    ply + 1,
                    -(alpha + 1),
                    -alpha,
                    new_depth - i32::from(r + r_extra > 3135),
                    !cut_node,
                );
            } else {
                value = alpha + 1;
            }

            // PV search
            if pv_node && (move_count == 1 || value > alpha) {
                if m == tt_data.tt_move && tt_data.depth > 1 {
                    new_depth = new_depth.max(1);
                }
                value = -self.ab_search::<false>(ply + 1, -beta, -alpha, new_depth, false);
            }

            // Undo move
            self.root_pos.undo_move(m);

            if self.stop.load(Ordering::Relaxed) {
                return VALUE_ZERO;
            }

            // Update root move
            if ROOT {
                let current_nodes = self.node_count();
                let sel_depth = self.sel_depth;
                let pv_idx = self.pv_idx;
                let bmc = self.best_move_changes.load(Ordering::Relaxed);
                if let Some(rm) = self.root_moves.iter_mut().find(|rm| rm.pv[0] == m) {
                    rm.effort += current_nodes - node_count;
                    rm.average_score = if rm.average_score == -VALUE_INFINITE {
                        value
                    } else {
                        (value + rm.average_score) / 2
                    };

                    if move_count == 1 || value > alpha {
                        rm.score = value;
                        rm.uci_score = value;
                        rm.sel_depth = sel_depth;
                        rm.score_lowerbound = false;
                        rm.score_upperbound = false;

                        if value >= beta {
                            rm.score_lowerbound = true;
                            rm.uci_score = beta;
                        } else if value <= alpha {
                            rm.score_upperbound = true;
                            rm.uci_score = alpha;
                        }

                        rm.pv.truncate(1);

                        if move_count > 1 && pv_idx == 0 {
                            self.best_move_changes.store(bmc + 1, Ordering::Relaxed);
                        }
                    } else {
                        rm.score = -VALUE_INFINITE;
                    }
                }
            }

            if value > best_value {
                best_value = value;
                if value > alpha {
                    best_move = m;
                    if value >= beta {
                        self.ss_cutoff_cnts[ss] += 1;
                        break;
                    }
                    if depth > 2 && depth < 11 && !is_decisive(value) {
                        depth -= 2;
                    }
                    alpha = value;
                }
            }

            if m != best_move && move_count <= SEARCHED_LIST_CAPACITY as i32 && !capture {
                quiets_searched.push(m);
            }
        }

        // Adjust best value for fail high
        if best_value >= beta && !is_decisive(best_value) && !is_decisive(alpha) {
            best_value = (best_value * depth + beta) / (depth + 1);
        }

        if move_count == 0 {
            best_value = if excluded_move.is_ok() {
                alpha
            } else {
                mated_in(ply)
            };
        }

        // Update history
        if best_move.is_ok() && !self.root_pos.is_capture(best_move) {
            let bonus = (162 * depth - 87).min(1602);
            self.main_history.update(us, best_move, bonus);
            if ply < LOW_PLY_HISTORY_SIZE as i32 {
                self.low_ply_history
                    .update(ply as usize, best_move, bonus * 693 / 1024);
            }
            let malus = (870 * depth - 148).min(2000);
            for &qm in &quiets_searched {
                self.main_history.update(us, qm, -malus);
            }
        }

        // Write to TT
        if !(excluded_move.is_ok() || ROOT && self.pv_idx > 0) {
            let bound = if best_value >= beta {
                Bound::Lower
            } else if pv_node && best_move.is_ok() {
                Bound::Exact
            } else {
                Bound::Upper
            };
            tt_writer.write(
                pos_key,
                value_to_tt(best_value, ply),
                tt_pv,
                bound,
                if move_count != 0 {
                    depth
                } else {
                    depth.min(MAX_PLY - 1)
                },
                best_move,
                unadjusted_static_eval,
                self.tt.generation(),
            );
        }

        best_value
    }
}
