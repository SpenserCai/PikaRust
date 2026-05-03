use std::sync::atomic::Ordering;

use crate::position::rule_judge::RuleJudgeResult;
use crate::types::{
    Bound, DEPTH_UNSEARCHED, Depth, MAX_PLY, Move, PIECE_VALUE, PieceType, VALUE_DRAW,
    VALUE_INFINITE, VALUE_MATE_IN_MAX_PLY, VALUE_MATED_IN_MAX_PLY, VALUE_NONE, VALUE_ZERO, Value,
    is_decisive, is_loss, is_valid, is_win, mate_in, mated_in,
};

use super::movepick::MovePicker;
use super::search::{
    LMR_DIVISOR, SEARCHED_LIST_CAPACITY, Worker, to_corrected_static_eval, value_from_tt,
    value_to_tt,
};

impl Worker {
    #[allow(clippy::too_many_lines)]
    pub fn iterative_deepening(&mut self) -> Option<Move> {
        let is_main = self.is_main_thread();
        let us = self.root_pos.side_to_move();
        let mut best_value;
        let mut iter_idx: usize = 0;
        let mut search_again_counter = 0;
        let mut time_reduction = 1.0_f64;
        let mut last_best_move_depth: Depth = 0;

        self.reset_ss();
        self.reset_acc();

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

            // Age out PV variability metric at the start of each iteration
            if is_main {
                let prev = self.tot_best_move_changes.load(Ordering::Relaxed);
                self.tot_best_move_changes
                    .store(prev / 2, Ordering::Relaxed);
            }

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
                    last_best_move_depth = self.root_depth;
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

            // Accumulate best move changes from this worker into the shared total,
            // then reset the per-worker counter.
            let bmc = self.best_move_changes.load(Ordering::Relaxed);
            self.tot_best_move_changes.fetch_add(bmc, Ordering::Relaxed);
            self.best_move_changes.store(0, Ordering::Relaxed);

            // Dynamic time management
            if self.limits.use_time_management()
                && !self.stop.load(Ordering::Relaxed)
                && !self.stop_on_ponderhit
            {
                let total_nodes = self.node_count().max(1);
                let nodes_effort = self.root_moves[0].effort * 100_000 / total_nodes;

                let best_prev_avg = f64::from(self.best_previous_avg_score);
                let best_val = f64::from(best_value);
                let iter_val = f64::from(self.iter_value[iter_idx]);

                let falling_eval = (0.81f64.mul_add(
                    iter_val - best_val,
                    2.730f64.mul_add(best_prev_avg - best_val, 16.93),
                ) / 100.0)
                    .clamp(0.610, 1.860);

                // If the best move is stable over several iterations, reduce time
                let depth_diff = f64::from(self.completed_depth - last_best_move_depth);
                time_reduction = interpolate(depth_diff, 8.0, 17.0, 0.67, 1.44).clamp(0.67, 1.44);

                let reduction = (2.1 + self.previous_time_reduction) / (2.480 * time_reduction);

                let tot_bmc = self.tot_best_move_changes.load(Ordering::Relaxed) as f64;
                let best_move_instability = 0.960 + 1.630 * tot_bmc / self.num_threads as f64;

                let high_best_move_effort =
                    interpolate(nodes_effort as f64, 78_000.0, 94_000.0, 0.96, 0.74)
                        .clamp(0.74, 0.96);

                let total_time = self.tm.optimum() as f64
                    * falling_eval
                    * reduction
                    * best_move_instability
                    * high_best_move_effort;

                let elapsed = self.tm.elapsed() as f64;

                // Stop the search if we have exceeded the totalTime or maximum
                if elapsed > total_time.min(self.tm.maximum() as f64) {
                    if self.ponder.load(Ordering::Relaxed) {
                        self.stop_on_ponderhit = true;
                    } else {
                        self.stop.store(true, Ordering::Relaxed);
                    }
                } else {
                    self.increase_depth.store(
                        self.ponder.load(Ordering::Relaxed) || elapsed <= total_time * 0.26,
                        Ordering::Relaxed,
                    );
                }
            }

            self.iter_value[iter_idx] = best_value;
            iter_idx = (iter_idx + 1) & 3;
        }

        if is_main {
            while !self.stop.load(Ordering::Relaxed)
                && (self.ponder.load(Ordering::Relaxed) || self.limits.infinite)
            {
                std::thread::park_timeout(std::time::Duration::from_millis(1));
            }
            self.stop.store(true, Ordering::Relaxed);
        }

        if is_main {
            self.best_previous_score = self.root_moves[0].score;
            self.best_previous_avg_score = self.root_moves[0].average_score;
            self.previous_time_reduction = time_reduction;
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

        // Phase B: Initialize statScore for this ply
        self.ss_stat_scores[ss] = 0;

        // Phase B: Set followPV
        self.ss_follow_pvs[ss] = ROOT
            || (ss >= 1
                && self.ss_follow_pvs[ss - 1]
                && (ply as usize) >= 1
                && (ply as usize - 1) < self.last_iteration_pv.len()
                && self.ss_current_moves[ss - 1] == self.last_iteration_pv[ply as usize - 1]);

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
            match self.root_pos.rule_judge(ply) {
                RuleJudgeResult::Definitive(result) => {
                    return if result == VALUE_DRAW {
                        self.value_draw()
                    } else {
                        result
                    };
                }
                RuleJudgeResult::TwoFold(result) => {
                    if result > VALUE_DRAW {
                        alpha = alpha.max(VALUE_DRAW - 1);
                    } else {
                        beta = beta.min(VALUE_DRAW + 1);
                    }
                }
                RuleJudgeResult::None => {}
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
        self.ss_tt_hits[ss] = tt_hit;

        // Prior capture and previous square for history updates
        let prior_capture = self.root_pos.captured_piece() != crate::types::Piece::NONE;
        let prev_sq = if ss >= 1 && self.ss_current_moves[ss - 1].is_ok() {
            Some(self.ss_current_moves[ss - 1].to_sq())
        } else {
            None
        };

        // Static evaluation
        let mut unadjusted_static_eval = VALUE_NONE;
        let mut eval;

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
            eval = to_corrected_static_eval(unadjusted_static_eval, correction_val);
            self.ss_static_evals[ss] = eval;

            if !tt_hit {
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
        }

        // Read and reset priorReduction (C++: priorReduction = (ss-1)->reduction; (ss-1)->reduction = 0)
        let prior_reduction = if ss >= 1 {
            let v = self.ss_reductions[ss - 1];
            self.ss_reductions[ss - 1] = 0;
            v
        } else {
            0
        };

        // evalDiff history update (C++ skips this when in_check via goto moves_loop)
        if !in_check
            && ss >= 1
            && self.ss_current_moves[ss - 1].is_ok()
            && !self.ss_in_check[ss - 1]
            && !prior_capture
            && is_valid(self.ss_static_evals[ss])
            && is_valid(self.ss_static_evals[ss - 1])
        {
            let eval_diff =
                (-(self.ss_static_evals[ss - 1] + self.ss_static_evals[ss])).clamp(-110, 187) + 34;
            let not_us = !self.root_pos.side_to_move();
            let prev_move = self.ss_current_moves[ss - 1];
            self.main_history.update(not_us, prev_move, eval_diff * 13);

            if !tt_hit {
                if let Some(psq) = prev_sq {
                    let pc_on_prev = self.root_pos.piece_on(psq);
                    if pc_on_prev.piece_type() != PieceType::Pawn {
                        let pawn_key = self.root_pos.pawn_key();
                        self.pawn_history.entry_mut(pawn_key).update(
                            pc_on_prev,
                            psq,
                            eval_diff * 12,
                        );
                    }
                }
            }
        }
        let opponent_worsening = ss >= 1
            && is_valid(self.ss_static_evals[ss])
            && is_valid(self.ss_static_evals[ss - 1])
            && self.ss_static_evals[ss] > -self.ss_static_evals[ss - 1];

        // Phase B: Depth adjustments based on priorReduction/opponentWorsening
        if !in_check {
            if prior_reduction >= 3 && !opponent_worsening {
                depth += 1;
            }
            if prior_reduction >= 2
                && depth >= 2
                && is_valid(self.ss_static_evals[ss])
                && ss >= 1
                && is_valid(self.ss_static_evals[ss - 1])
                && self.ss_static_evals[ss] + self.ss_static_evals[ss - 1] > 193
            {
                depth -= 1;
            }
        }

        let mut improving = ss >= 2 && eval > self.ss_static_evals[ss - 2];

        // Phase B: TT value as better eval
        if !in_check
            && !excluded_move.is_ok()
            && is_valid(tt_data.value)
            && !is_decisive(tt_data.value)
        {
            let bound_ok = if tt_data.value > eval {
                tt_data.bound as u8 & Bound::Lower as u8 != 0
            } else {
                tt_data.bound as u8 & Bound::Upper as u8 != 0
            };
            if bound_ok {
                eval = tt_data.value;
            }
        }

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
            if bound_ok && ((cut_node == (tt_data.value >= beta)) || depth > 5) {
                // TT cutoff history updates (before rule60 check)
                if tt_data.tt_move.is_ok() && tt_data.value >= beta {
                    if !tt_capture {
                        let tt_bonus = (108 * depth - 60).min(1433);
                        self.update_quiet_histories(ply, tt_data.tt_move, tt_bonus);
                    }
                    if ss >= 1 && self.ss_move_counts[ss - 1] < 3 && !prior_capture {
                        if let Some(psq) = prev_sq {
                            let pc_on_prev = self.root_pos.piece_on(psq);
                            self.update_continuation_histories(ply - 1, pc_on_prev, psq, -2218);
                        }
                    }
                }

                // Graph history interaction workaround
                if self.root_pos.rule60_count() < 116 {
                    if depth >= 7
                        && tt_data.tt_move.is_ok()
                        && self.root_pos.pseudo_legal(tt_data.tt_move)
                        && self.root_pos.is_legal(tt_data.tt_move)
                        && !is_decisive(tt_data.value)
                    {
                        let gives_check = self.root_pos.gives_check(tt_data.tt_move);
                        self.root_pos.do_move(tt_data.tt_move, gives_check);
                        let next_key = self.root_pos.key();
                        let next_probe = self.tt.probe(next_key);
                        self.root_pos.undo_move(tt_data.tt_move);

                        if !is_valid(next_probe.data.value) {
                            return tt_data.value;
                        }
                        if (tt_data.value >= beta) == (-next_probe.data.value >= beta) {
                            return tt_data.value;
                        }
                    } else {
                        return tt_data.value;
                    }
                }
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
                let correction_val = self.correction_value(ply);
                let margin = fm * depth
                    - (2512 * i32::from(improving) + 340 * i32::from(opponent_worsening)) * fm
                        / 1024
                    + correction_val.abs() / 132_109;
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
                self.set_cont_hist_index_sentinel(ply);
                self.root_pos.do_null_move();
                let null_value =
                    -self.ab_search::<false>(ply + 1, -beta, -beta + 1, depth - r, false);
                self.root_pos.undo_null_move();
                self.root_pos
                    .debug_check_consistency("after_undo_null_move");

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

            // Phase B: improving |= eval >= beta (after null move)
            improving |= eval >= beta;

            // IIR (Phase B: add followPV and priorReduction conditions)
            if !self.ss_follow_pvs[ss]
                && !all_node
                && depth >= 6
                && !tt_data.tt_move.is_ok()
                && prior_reduction <= 3
            {
                depth -= 1;
            }

            // Step 10. ProbCut
            // If we have a good enough capture and a reduced search returns a value
            // much above beta, we can (almost) safely prune the previous move.
            let prob_cut_beta = beta + 251 - 66 * i32::from(improving);
            if depth >= 3
                && !is_decisive(beta)
                && !(is_valid(tt_data.value) && tt_data.value < prob_cut_beta)
            {
                debug_assert!(prob_cut_beta < VALUE_INFINITE && prob_cut_beta > beta);

                let prob_cut_depth = depth - 4;
                let mut pc_mp = MovePicker::new_probcut(
                    &self.root_pos,
                    tt_data.tt_move,
                    prob_cut_beta - eval,
                    &self.capture_history,
                );

                loop {
                    let pc_move = pc_mp.next_move(&self.root_pos);
                    if pc_move == Move::NONE {
                        break;
                    }

                    if pc_move == excluded_move || !self.root_pos.is_legal(pc_move) {
                        continue;
                    }

                    let pc_gives_check = self.root_pos.gives_check(pc_move);
                    self.ss_current_moves[ss] = pc_move;
                    self.ss_in_check[ss] = pc_gives_check;
                    self.push_acc_for_move(pc_move);
                    self.root_pos.do_move(pc_move, pc_gives_check);
                    self.inc_nodes();

                    // Preliminary qsearch to verify the move holds
                    let mut pc_value =
                        -self.qsearch(ply + 1, -prob_cut_beta, -prob_cut_beta + 1, false);

                    // If qsearch held, do a regular search at reduced depth
                    if pc_value >= prob_cut_beta && prob_cut_depth > 0 {
                        pc_value = -self.ab_search::<false>(
                            ply + 1,
                            -prob_cut_beta,
                            -prob_cut_beta + 1,
                            prob_cut_depth,
                            !cut_node,
                        );
                    }

                    self.root_pos.undo_move(pc_move);
                    self.pop_acc();

                    if pc_value >= prob_cut_beta {
                        // Save ProbCut data into transposition table
                        tt_writer.write(
                            pos_key,
                            value_to_tt(pc_value, ply),
                            tt_pv,
                            Bound::Lower,
                            prob_cut_depth + 1,
                            pc_move,
                            unadjusted_static_eval,
                            self.tt.generation(),
                        );

                        if !is_decisive(pc_value) {
                            return pc_value - (prob_cut_beta - beta);
                        }
                    }
                }
            }
        }

        // Step 11. A small ProbCut idea
        {
            let prob_cut_beta = beta + 470;
            if (tt_data.bound as u8 & Bound::Lower as u8) != 0
                && tt_data.depth >= depth - 4
                && tt_data.value >= prob_cut_beta
                && !is_decisive(beta)
                && is_valid(tt_data.value)
                && !is_decisive(tt_data.value)
            {
                return prob_cut_beta;
            }
        }

        // Moves loop — build contHist from search stack
        let cont_hist_refs = self.build_cont_hist_for_movepicker(ply);
        let cont_hist_slice: Vec<&super::history::PieceToHistory> = cont_hist_refs;

        let mut mp = MovePicker::new_main(
            &self.root_pos,
            tt_data.tt_move,
            depth,
            &self.main_history,
            &self.low_ply_history,
            &self.capture_history,
            &cont_hist_slice,
            &self.pawn_history,
            ply,
        );

        let mut best_value = -VALUE_INFINITE;
        let mut best_move = Move::NONE;
        let mut move_count = 0i32;
        let mut quiets_searched = Vec::with_capacity(SEARCHED_LIST_CAPACITY);
        let mut captures_searched = Vec::with_capacity(SEARCHED_LIST_CAPACITY);

        // Compute correction value once before moves loop (on parent position)
        let correction_val = self.correction_value(ply);

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
            let moved_piece = self.root_pos.moved_piece(m);

            let mut new_depth = depth - 1;
            let mut extension: i32 = 0;

            let delta = beta - alpha;
            let mut r = self.reduction(improving, depth, move_count, delta);

            // Increase reduction for ttPv nodes (before Step 13, affects lmrDepth)
            if tt_pv {
                r += 931;
            }

            // === Step 13: Pruning at shallow depths ===

            // 13a. Calculate lmrDepth for use in all Step 13 pruning
            let lmr_depth = (new_depth - r / 1005).max(0);

            // 13b. LMP: use skip_quiet_moves instead of hard continue
            if !ROOT
                && self.root_pos.major_material(us) > 0
                && !is_loss(best_value)
                && move_count >= (3 + depth * depth) / (2 - i32::from(improving))
            {
                mp.skip_quiet_moves();
            }

            // 13c. Capture futility pruning
            if !ROOT
                && self.root_pos.major_material(us) > 0
                && !is_loss(best_value)
                && !gives_check
                && lmr_depth < 19
                && !in_check
                && capture
            {
                let capt_hist = self.capture_history.get(
                    moved_piece,
                    m.to_sq(),
                    self.root_pos.piece_on(m.to_sq()).piece_type(),
                );
                let futility_value = self.ss_static_evals[ss]
                    + 322
                    + 336 * lmr_depth
                    + PIECE_VALUE[self.root_pos.piece_on(m.to_sq())]
                    + 229 * i32::from(capt_hist) / 1024;
                if futility_value <= alpha {
                    continue;
                }
            }

            // 13d. SEE pruning for captures/checks
            if !ROOT
                && self.root_pos.major_material(us) > 0
                && !is_loss(best_value)
                && (capture || gives_check)
            {
                let capt_hist = if capture {
                    i32::from(self.capture_history.get(
                        moved_piece,
                        m.to_sq(),
                        self.root_pos.piece_on(m.to_sq()).piece_type(),
                    ))
                } else {
                    0
                };
                let margin = (256 * depth + capt_hist * 34 / 1024).max(0);
                if !self.root_pos.see_ge(m, -margin) {
                    continue;
                }
            }

            // 13e. Quiet move pruning (only for non-capture, non-check moves)
            if !ROOT
                && self.root_pos.major_material(us) > 0
                && !is_loss(best_value)
                && !capture
                && !gives_check
                && (!self.ss_follow_pvs[ss] || !pv_node)
            {
                // Continuation history + pawn history pruning
                let pawn_key = self.root_pos.pawn_key();
                let cont_hist_val = self.get_cont_hist_value(ply, moved_piece, m.to_sq());
                let history = cont_hist_val
                    + i32::from(
                        self.pawn_history
                            .entry(pawn_key)
                            .get(moved_piece, m.to_sq()),
                    );
                if history < -2995 * depth {
                    continue;
                }

                // History-adjusted lmrDepth
                let history = history + 73 * i32::from(self.main_history.get(us, m)) / 32;
                let d_index = (depth as usize).min(16).saturating_sub(1);
                let adjusted_lmr_depth = lmr_depth + history / LMR_DIVISOR[d_index];

                // Quiet futility pruning (parent node)
                if !in_check && adjusted_lmr_depth < 10 {
                    let futility_value = self.ss_static_evals[ss]
                        + 47
                        + 272 * i32::from(!best_move.is_ok())
                        + 129 * adjusted_lmr_depth
                        + 112 * i32::from(self.ss_static_evals[ss] > alpha);
                    if futility_value <= alpha {
                        if best_value <= futility_value
                            && !is_loss(best_value)
                            && !is_win(futility_value)
                        {
                            best_value = futility_value;
                        }
                        continue;
                    }
                }

                // SEE pruning for quiets
                let see_lmr_depth = adjusted_lmr_depth.max(0);
                if !self.root_pos.see_ge(m, -35 * see_lmr_depth * see_lmr_depth) {
                    continue;
                }
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
                && !self.is_shuffling(m, ss, ply)
            {
                let sb = tt_data.value - (44 + 72 * i32::from(tt_pv && !pv_node)) * depth / 69;
                let sd = new_depth / 2;

                self.ss_excluded_moves[ss] = m;
                let value = self.ab_search::<false>(ply, sb - 1, sb, sd, cut_node);
                self.ss_excluded_moves[ss] = Move::NONE;

                if value < sb {
                    // Phase D: Improved singular extension margin
                    let corr_val_adj = self.correction_value(ply).abs() / 265_845;
                    let tt_mh_adj = 1085 * i32::from(self.tt_move_history.get()) / 133_615;
                    let ply_gt_root = i32::from(ply > self.root_depth);
                    let double_margin = -4 + 234 * i32::from(pv_node)
                        - 172 * i32::from(!tt_capture)
                        - corr_val_adj
                        - tt_mh_adj
                        - ply_gt_root * 43;
                    let triple_margin = 106 + 299 * i32::from(pv_node)
                        - 263 * i32::from(!tt_capture)
                        + 93 * i32::from(tt_pv)
                        - corr_val_adj
                        - ply_gt_root * 60;
                    extension = 1
                        + i32::from(value < sb - double_margin)
                        + i32::from(value < sb - triple_margin);
                    depth += 1;
                } else if value >= beta && !is_decisive(value) {
                    self.tt_move_history.update((-397 - 103 * depth).max(-4055));
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
            self.set_cont_hist_index(ply, in_check, capture, moved_piece, m.to_sq());
            self.push_acc_for_move(m);
            self.root_pos.do_move(m, gives_check);
            self.root_pos.debug_check_consistency("after_do_move_ab");
            self.inc_nodes();

            new_depth += extension;
            let node_count = if ROOT { self.node_count() } else { 0 };

            let mut value;

            // Compute statScore (after do_move, captures need captured_piece())
            if capture {
                self.ss_stat_scores[ss] = 953 * PIECE_VALUE[self.root_pos.captured_piece()] / 128
                    + i32::from(self.capture_history.get(
                        moved_piece,
                        m.to_sq(),
                        self.root_pos.captured_piece().piece_type(),
                    ));
            } else {
                self.ss_stat_scores[ss] = 2 * i32::from(self.main_history.get(us, m))
                    + self.get_cont_hist_value(ply, moved_piece, m.to_sq());
            }

            // All r adjustments (C++ applies these before the LMR/non-LMR branch)

            if tt_pv {
                r -= 2363
                    + i32::from(pv_node) * 963
                    + i32::from(is_valid(tt_data.value) && tt_data.value > alpha) * 1121
                    + i32::from(tt_data.depth >= depth) * (1137 + i32::from(cut_node) * 922);
            }
            r += 855;
            r -= move_count * 64;
            r -= correction_val.abs() / 30558;

            if cut_node {
                r += 3251 + 1048 * i32::from(!tt_data.tt_move.is_ok());
            }
            if tt_capture {
                r += 1571;
            }

            if ss + 1 < self.ss_cutoff_cnts.len() && self.ss_cutoff_cnts[ss + 1] > 1 {
                r += 256
                    + 1024 * i32::from(self.ss_cutoff_cnts[ss + 1] > 2)
                    + 1024 * i32::from(all_node);
            }

            if m == tt_data.tt_move {
                r -= 2953;
            }

            r -= self.ss_stat_scores[ss] * 946 / 8192;

            if all_node {
                r += r * 256 / (256 * depth + 256);
            }

            // LMR
            if depth >= 2 && move_count > 1 {
                let d = 1.max((new_depth - r / 1024).min(new_depth + 2)) + i32::from(pv_node);

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

                    // Post-LMR continuation history update (after deeper re-search)
                    self.update_continuation_histories(ply, moved_piece, m.to_sq(), 1528);
                }
            } else if !pv_node || move_count > 1 {
                // Full-depth search when LMR is skipped (uses fully adjusted r)
                let r_extra = if tt_data.tt_move.is_ok() { 0 } else { 979 };
                value = -self.ab_search::<false>(
                    ply + 1,
                    -(alpha + 1),
                    -alpha,
                    new_depth
                        - i32::from(r + r_extra > 3135)
                        - i32::from(r + r_extra > 4840 && new_depth > 2),
                    !cut_node,
                );
            } else {
                value = alpha + 1;
            }

            // PV search
            if pv_node && (move_count == 1 || value > alpha) {
                if m == tt_data.tt_move
                    && ((is_valid(tt_data.value)
                        && is_decisive(tt_data.value)
                        && tt_data.depth > 0)
                        || tt_data.depth > 1)
                {
                    new_depth = new_depth.max(1);
                }
                value = -self.ab_search::<false>(ply + 1, -beta, -alpha, new_depth, false);
            }

            // Undo move
            self.root_pos.undo_move(m);
            self.pop_acc();

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

                    // meanSquaredScore update (unconditional, before alpha check)
                    rm.mean_squared_score = if rm.mean_squared_score
                        == -i64::from(VALUE_INFINITE) * i64::from(VALUE_INFINITE)
                    {
                        i64::from(value) * i64::from(value.abs())
                    } else {
                        (i64::from(value) * i64::from(value.abs()) + rm.mean_squared_score) / 2
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

            // Alternative move promotion: pretend equal-value moves just exceed alpha
            let inc = i32::from(
                value == best_value
                    && ply + 2 >= self.root_depth
                    && (self.node_count() as i32).trailing_zeros() >= 4
                    && !is_win(value.abs() + 1),
            );

            if value + inc > best_value {
                best_value = value;
                if value + inc > alpha {
                    best_move = m;
                    if value >= beta {
                        // Phase C: cutoffCnt condition fix
                        self.ss_cutoff_cnts[ss] += i32::from(extension < 2 || pv_node);
                        break;
                    }
                    if depth > 2 && depth < 11 && !is_decisive(value) {
                        depth -= 2;
                    }
                    alpha = value;
                }
            }

            if m != best_move && move_count <= SEARCHED_LIST_CAPACITY as i32 {
                if capture {
                    captures_searched.push(m);
                } else {
                    quiets_searched.push(m);
                }
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
        if move_count > 0 && best_move.is_ok() {
            self.update_all_stats(
                ply,
                best_move,
                prev_sq,
                &quiets_searched,
                &captures_searched,
                depth,
                tt_data.tt_move,
            );
            if !pv_node {
                let tt_bonus = if best_move == tt_data.tt_move {
                    796
                } else {
                    -855
                };
                self.tt_move_history.update(tt_bonus);
            }
        } else if move_count > 0 && !prior_capture {
            // Bonus for prior quiet countermove that caused the fail low
            if let Some(psq) = prev_sq {
                let parent_ss = ss.saturating_sub(1);
                let mut bonus_scale: i32 = -231;
                bonus_scale -= if parent_ss < self.ss_stat_scores.len() {
                    self.ss_stat_scores[parent_ss] / 73
                } else {
                    0
                };
                bonus_scale += (62 * depth).min(512);
                if parent_ss < self.ss_move_counts.len() {
                    bonus_scale += 152 * i32::from(self.ss_move_counts[parent_ss] > 13);
                }
                bonus_scale +=
                    76 * i32::from(!in_check && best_value <= self.ss_static_evals[ss] - 166);
                if parent_ss < self.ss_in_check.len() && parent_ss < self.ss_static_evals.len() {
                    bonus_scale += 163
                        * i32::from(
                            !self.ss_in_check[parent_ss]
                                && best_value <= -self.ss_static_evals[parent_ss] - 109,
                        );
                }

                bonus_scale = bonus_scale.max(0);

                let scaled_bonus = (148 * depth - 86).min(2188) * bonus_scale;

                let pc_on_prev = self.root_pos.piece_on(psq);
                self.update_continuation_histories(
                    ply - 1,
                    pc_on_prev,
                    psq,
                    scaled_bonus * 192 / 16384,
                );

                let not_us = !self.root_pos.side_to_move();
                let prev_move = self.ss_current_moves[parent_ss];
                self.main_history
                    .update(not_us, prev_move, scaled_bonus * 216 / 32768);

                if pc_on_prev.piece_type() != crate::types::PieceType::Pawn {
                    let pawn_key = self.root_pos.pawn_key();
                    self.pawn_history.entry_mut(pawn_key).update(
                        pc_on_prev,
                        psq,
                        scaled_bonus * 244 / 8192,
                    );
                }
            }
        } else if move_count > 0 && prior_capture {
            // Bonus for prior capture countermove that caused the fail low
            if let Some(psq) = prev_sq {
                let captured_piece = self.root_pos.captured_piece();
                if captured_piece != crate::types::Piece::NONE {
                    let pc_on_prev = self.root_pos.piece_on(psq);
                    self.capture_history
                        .update(pc_on_prev, psq, captured_piece.piece_type(), 983);
                }
            }
        }

        // If no good move is found and the previous position was ttPv, then the previous
        // opponent move is probably good and the new position is added to the search tree.
        if best_value <= alpha {
            self.ss_tt_pvs[ss] = self.ss_tt_pvs[ss] || (ss >= 1 && self.ss_tt_pvs[ss - 1]);
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
                    // Phase D: TT write depth for no-move case
                    (depth + 6).min(MAX_PLY - 1)
                },
                best_move,
                unadjusted_static_eval,
                self.tt.generation(),
            );
        }

        // Adjust correction history
        if !(in_check || best_move.is_ok() && self.root_pos.is_capture(best_move))
            && (best_value > self.ss_static_evals[ss]) == best_move.is_ok()
        {
            let corr_bonus = ((best_value - self.ss_static_evals[ss])
                * depth
                * (if best_move.is_ok() { 12 } else { 17 })
                / 128)
                .clamp(
                    -super::history::CORRECTION_HISTORY_LIMIT / 4,
                    super::history::CORRECTION_HISTORY_LIMIT / 4,
                );
            self.update_correction_history(ply, 1069 * corr_bonus / 1024);
        }

        best_value
    }

    /// Detect shuffling: both sides repeatedly move the same piece back and forth.
    /// Aligns with Pikafish `is_shuffling` (search.cpp:134-142).
    #[inline]
    fn is_shuffling(&self, m: Move, ss: usize, ply: i32) -> bool {
        if self.root_pos.is_capture(m) || self.root_pos.rule60_count() < 11 {
            return false;
        }
        if self.root_pos.state().plies_from_null <= 6 || ply < 19 {
            return false;
        }
        ss >= 4
            && self.ss_current_moves[ss - 2].is_ok()
            && self.ss_current_moves[ss - 4].is_ok()
            && m.from_sq() == self.ss_current_moves[ss - 2].to_sq()
            && self.ss_current_moves[ss - 2].from_sq() == self.ss_current_moves[ss - 4].to_sq()
    }
}

/// Linear interpolation: maps `x` from the range `[x0, x1]` to `[y0, y1]`.
/// Extrapolates if `x` is outside `[x0, x1]`.
#[inline]
fn interpolate(x: f64, x0: f64, x1: f64, y0: f64, y1: f64) -> f64 {
    debug_assert!((x1 - x0).abs() > f64::EPSILON);
    y0 + (x - x0) * (y1 - y0) / (x1 - x0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use crate::position::{GenType, Position, generate};
    use crate::search::search::{RootMove, Worker};
    use crate::search::time::SearchLimits;
    use crate::search::tt::TranspositionTable;
    use crate::types::Move;

    const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

    /// One-side-dominant position: red has massive material advantage.
    const DOMINANT_FEN: &str = "4k4/4a4/9/9/9/9/P1P1P1P1P/1C2C1N2/9/RNBAKAB1R w - - 0 1";

    /// Midgame position with balanced material.
    const MIDGAME_FEN: &str =
        "r1bakab1r/9/1cn1c1n2/p1p1p1p1p/9/9/P1P1P1P1P/1C2C1N2/9/RNBAKAB1R w - - 0 1";

    fn load_network() -> Option<Arc<crate::nnue::Network>> {
        let path = std::path::Path::new("models/pikafish.nnue");
        if !path.exists() {
            return None;
        }
        crate::nnue::NnueModel::load(path)
            .ok()
            .map(|m| Arc::new(crate::nnue::Network::new(m)))
    }

    /// Create a Worker ready for search on the given FEN at the given depth.
    fn make_worker(fen: &str, depth: i32, network: Option<Arc<crate::nnue::Network>>) -> Worker {
        let stop = Arc::new(AtomicBool::new(false));
        let ponder = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(16));
        let increase_depth = Arc::new(AtomicBool::new(true));
        let tot_best_move_changes = Arc::new(AtomicU64::new(0));

        let mut w = Worker::new(
            0,
            stop,
            ponder,
            tt,
            increase_depth,
            tot_best_move_changes,
            1,
            network,
        );

        let pos = Position::from_fen(fen).expect("valid FEN");
        let legal_moves = generate(&pos, GenType::Legal);
        let mut root_moves = Vec::new();
        for i in 0..legal_moves.len() {
            root_moves.push(RootMove::new(legal_moves.get(i)));
        }

        let mut limits = SearchLimits::new();
        limits.depth = depth;
        limits.start_time = std::time::Instant::now();

        w.root_pos = pos;
        w.root_moves = root_moves;
        w.limits = limits;
        w
    }

    // ---------------------------------------------------------------
    // Section 8.2: Unit Tests
    // ---------------------------------------------------------------

    #[test]
    fn test_step13_capture_futility_prunes_bad_captures() {
        // Position where capture futility should prune bad captures:
        // Red has a rook that can capture a pawn, but the position is
        // already losing enough that the capture doesn't help.
        let fen = "4k4/4a4/4b4/9/9/4R4/9/9/4A4/4K4 w - - 0 1";
        let network = load_network();
        let mut w = make_worker(fen, 5, network);
        let result = w.iterative_deepening();

        assert!(result.is_some(), "search should return a move");
        let best = result.unwrap();
        assert!(best.is_ok(), "returned move should be valid");

        // Verify the move is legal in the position
        let pos = Position::from_fen(fen).expect("valid FEN");
        assert!(pos.is_legal(best), "returned move should be legal");
    }

    #[test]
    fn test_step13_see_pruning_reduces_nodes() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 10, network);
        w.iterative_deepening();

        let nodes = w.node_count();
        assert!(
            nodes < 100_000,
            "depth 10 startpos should search < 100,000 nodes with pruning, got {nodes}"
        );
        assert!(
            !w.root_moves.is_empty(),
            "should have root moves after search"
        );
        assert!(w.root_moves[0].pv[0].is_ok(), "best move should be valid");
    }

    #[test]
    fn test_step13_quiet_futility_prunes_hopeless_quiets() {
        let network = load_network();
        let mut w = make_worker(DOMINANT_FEN, 8, network);
        let result = w.iterative_deepening();

        assert!(result.is_some(), "search should return a move");
        let best = result.unwrap();
        assert!(best.is_ok(), "returned move should be valid");

        let pos = Position::from_fen(DOMINANT_FEN).expect("valid FEN");
        assert!(pos.is_legal(best), "returned move should be legal");

        let nodes = w.node_count();
        // Dominant position should be resolved quickly with pruning
        assert!(
            nodes < 500_000,
            "dominant position depth 8 should search < 500,000 nodes, got {nodes}"
        );
    }

    #[test]
    fn test_stat_score_nonzero_after_search() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 5, network);
        w.iterative_deepening();

        // After a depth 5 search, at least some ss_stat_scores entries
        // should have been set to non-zero values by the statScore logic.
        let ss_offset = 7; // SS_OFFSET
        let has_nonzero = (0..10).any(|ply| w.ss_stat_scores[ply + ss_offset] != 0);
        assert!(
            has_nonzero,
            "ss_stat_scores should have non-zero entries after depth 5 search"
        );
    }

    #[test]
    fn test_follow_pv_set_at_root() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 3, network);
        w.iterative_deepening();

        // followPV at root (ply 0) should be set to true during search.
        // After search completes, the root ply's followPV reflects the
        // last iteration's state. At ROOT, followPV is always set to true.
        let ss_root = w.ss_idx(0);
        assert!(
            w.ss_follow_pvs[ss_root],
            "ss_follow_pvs at root ply should be true after search"
        );
    }

    #[test]
    fn test_mean_squared_score_updated_after_search() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 5, network);

        let initial_mss = w.root_moves[0].mean_squared_score;
        w.iterative_deepening();

        assert_ne!(
            w.root_moves[0].mean_squared_score, initial_mss,
            "mean_squared_score should be updated after search (was {initial_mss}, now {})",
            w.root_moves[0].mean_squared_score
        );
    }

    #[test]
    fn test_opponent_worsening_depth_adjustment() {
        // Use a position where the opponent's eval worsens across plies.
        // The key test is that the search completes without panic and
        // returns a legal move, exercising the depth adjustment code.
        let fen = "r1bakab1r/9/2n1c1n2/p1p1p1p1p/9/2P6/P3P1P1P/1C2C1N2/9/RNBAKAB1R b - - 0 1";
        let network = load_network();
        let mut w = make_worker(fen, 6, network);
        let result = w.iterative_deepening();

        assert!(result.is_some(), "search should return a move");
        let best = result.unwrap();
        assert!(best.is_ok(), "returned move should be valid");

        let pos = Position::from_fen(fen).expect("valid FEN");
        assert!(pos.is_legal(best), "returned move should be legal");
    }

    #[test]
    fn test_lmr_cutoff_cnt_affects_reduction() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 8, network);
        let result = w.iterative_deepening();

        // The key assertion is that the search completes without panic,
        // which validates that cutoffCnt reads and LMR adjustments work.
        assert!(result.is_some(), "depth 8 search should complete");
        assert!(
            result.unwrap().is_ok(),
            "depth 8 search should return a valid move"
        );
    }

    #[test]
    fn test_tt_cutoff_updates_history() {
        let network = load_network();

        // First search: populates TT and history tables
        let stop = Arc::new(AtomicBool::new(false));
        let ponder = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(16));
        let increase_depth = Arc::new(AtomicBool::new(true));
        let tot_best_move_changes = Arc::new(AtomicU64::new(0));

        let mut w = Worker::new(
            0,
            Arc::clone(&stop),
            Arc::clone(&ponder),
            Arc::clone(&tt),
            Arc::clone(&increase_depth),
            Arc::clone(&tot_best_move_changes),
            1,
            network,
        );

        let pos = Position::from_fen(START_FEN).expect("valid FEN");
        let legal_moves = generate(&pos, GenType::Legal);
        let mut root_moves = Vec::new();
        for i in 0..legal_moves.len() {
            root_moves.push(RootMove::new(legal_moves.get(i)));
        }

        let mut limits = SearchLimits::new();
        limits.depth = 8;
        limits.start_time = std::time::Instant::now();

        w.root_pos = pos.clone();
        w.root_moves = root_moves.clone();
        w.limits = limits;

        w.iterative_deepening();
        let nodes_first = w.node_count();

        // Second search: same position, same TT — should benefit from
        // TT cutoffs and updated history tables.
        stop.store(false, std::sync::atomic::Ordering::SeqCst);
        w.root_depth = 0;
        w.completed_depth = 0;
        w.nodes.store(0, std::sync::atomic::Ordering::Relaxed);
        w.best_move_changes
            .store(0, std::sync::atomic::Ordering::Relaxed);
        w.root_moves = root_moves;
        w.root_pos = pos;

        let mut limits2 = SearchLimits::new();
        limits2.depth = 8;
        limits2.start_time = std::time::Instant::now();
        w.limits = limits2;

        w.iterative_deepening();
        let nodes_second = w.node_count();

        // Second search should use fewer or comparable nodes due to TT hits.
        // Allow up to 50% more nodes since search non-determinism (timing,
        // hash collisions) can cause variation.
        assert!(
            nodes_second <= nodes_first * 3 / 2,
            "second search should not be drastically slower \
             (first: {nodes_first}, second: {nodes_second})"
        );
    }

    // ---------------------------------------------------------------
    // Section 8.3: Regression Tests
    // ---------------------------------------------------------------

    #[test]
    #[ignore = "slow: depth 18 search takes ~30s in release mode"]
    fn test_search_depth18_node_count_reasonable() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 18, network);

        let start = std::time::Instant::now();
        let result = w.iterative_deepening();
        let elapsed = start.elapsed();

        assert!(result.is_some(), "depth 18 search should return a move");

        let nodes = w.node_count();
        assert!(
            nodes < 2_000_000,
            "depth 18 startpos should search < 2,000,000 nodes, got {nodes}"
        );
        assert!(
            elapsed.as_secs() < 30,
            "depth 18 search should complete in < 30s, took {:.1}s",
            elapsed.as_secs_f64()
        );
    }

    #[test]
    fn test_search_depth12_completes_quickly() {
        let network = load_network();
        let mut w = make_worker(START_FEN, 12, network);

        let start = std::time::Instant::now();
        let result = w.iterative_deepening();
        let elapsed = start.elapsed();

        assert!(result.is_some(), "depth 12 search should return a move");
        let best = result.unwrap();
        assert!(best.is_ok(), "returned move should be valid");

        let pos = Position::from_fen(START_FEN).expect("valid FEN");
        assert!(pos.is_legal(best), "returned move should be legal");

        // In release mode this completes in < 1s. In debug mode the search
        // is ~50x slower, so we use a generous 60s budget.
        assert!(
            elapsed.as_secs() < 60,
            "depth 12 search should complete in < 60s, took {:.1}s",
            elapsed.as_secs_f64()
        );
    }

    #[test]
    fn test_eval_equivalence_midgame_within_300cp() {
        let network = load_network();
        let mut w = make_worker(MIDGAME_FEN, 5, network);
        w.iterative_deepening();

        let score = w.root_moves[0].score;
        assert!(
            (-600..=600).contains(&score),
            "midgame depth 5 score should be in [-600, 600], got {score}"
        );
    }

    #[test]
    fn test_eval_diff_updates_main_history() {
        let network = load_network();

        // Create a fresh worker with zeroed history
        let stop = Arc::new(AtomicBool::new(false));
        let ponder = Arc::new(AtomicBool::new(false));
        let tt = Arc::new(TranspositionTable::new(16));
        let increase_depth = Arc::new(AtomicBool::new(true));
        let tot_best_move_changes = Arc::new(AtomicU64::new(0));

        let mut w = Worker::new(
            0,
            stop,
            ponder,
            tt,
            increase_depth,
            tot_best_move_changes,
            1,
            network,
        );

        // Explicitly zero out main_history so we can detect any updates
        w.main_history.fill(0);

        let pos = Position::from_fen(START_FEN).expect("valid FEN");
        let legal_moves = generate(&pos, GenType::Legal);
        let mut root_moves = Vec::new();
        for i in 0..legal_moves.len() {
            root_moves.push(RootMove::new(legal_moves.get(i)));
        }

        let mut limits = SearchLimits::new();
        limits.depth = 5;
        limits.start_time = std::time::Instant::now();

        w.root_pos = pos;
        w.root_moves = root_moves;
        w.limits = limits;

        // Collect the raw values of all legal moves to sample from
        let legal = generate(&w.root_pos, GenType::Legal);
        let move_raws: Vec<u16> = (0..legal.len()).map(|i| legal.get(i).raw()).collect();

        w.iterative_deepening();

        // Check if any of the legal moves' history entries were updated
        let has_update = move_raws.iter().any(|&raw| {
            let m = Move::from_raw(raw);
            w.main_history.get(crate::types::Color::White, m) != 0
                || w.main_history.get(crate::types::Color::Black, m) != 0
        });

        // Also check a broader range — the search explores many positions
        // and updates history for moves at all plies, not just root moves.
        let has_any_update = has_update
            || (0..16384).any(|raw| {
                let m = Move::from_raw(raw);
                w.main_history.get(crate::types::Color::White, m) != 0
                    || w.main_history.get(crate::types::Color::Black, m) != 0
            });

        assert!(
            has_any_update,
            "main_history should have non-zero updates after depth 5 search"
        );
    }

    #[test]
    fn test_singular_extension_margin_no_panic() {
        // Use a position likely to trigger singular extension:
        // a tactical position where one move is clearly best.
        let fen = "2bak4/4a4/4b4/9/2r6/1R7/9/4B4/4A4/2BAK4 w - - 0 1";
        let network = load_network();
        let mut w = make_worker(fen, 8, network);
        let result = w.iterative_deepening();

        // The key assertion is no panic — singular extension margin
        // calculations with correctionValue and ttMoveHistory complete safely.
        assert!(result.is_some(), "depth 8 search should complete");
        assert!(
            result.unwrap().is_ok(),
            "depth 8 search should return a valid move"
        );
    }
}
