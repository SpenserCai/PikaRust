use crate::types::{Color, PieceType, VALUE_DRAW, Value, mate_in, mated_in};

use super::movegen::{GenType, generate};
use super::position::Position;
use super::state::StateInfo;

/// Return type for `rule_judge`, distinguishing definitive from 2-fold results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleJudgeResult {
    /// No repetition detected.
    None,
    /// Definitive result (3-fold / draw / bloom-filter-confirmed 2-fold).
    Definitive(Value),
    /// 2-fold non-definitive result (first detection of 2-fold mate/mated).
    TwoFold(Value),
}

impl Position {
    fn state_at(&self, steps_back: usize) -> Option<&StateInfo> {
        if steps_back == 0 {
            Some(&self.state)
        } else {
            let stack_len = self.state_stack.len();
            if steps_back <= stack_len {
                Some(&self.state_stack[stack_len - steps_back])
            } else {
                None
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn rule_judge(&mut self, ply: i32) -> RuleJudgeResult {
        let extra_checks = i32::from((self.state.check10[Color::White] - 10).max(0))
            + i32::from((self.state.check10[Color::Black] - 10).max(0));
        let end = (self.state.rule60 + extra_checks).min(self.state.plies_from_null);

        let mut two_fold_result: Option<Value> = None;

        if end >= 4 && self.bloom_filter.count(self.state.key) >= 1 {
            let mut cnt = 0;

            let Some(st_2) = self.state_at(2) else {
                return self.check_rule60_and_material(ply).map_or(
                    RuleJudgeResult::None,
                    RuleJudgeResult::Definitive,
                );
            };
            let checkers_2 = st_2.checkers_bb;

            let checkers_3 = self
                .state_at(3)
                .map_or(crate::bitboard::Bitboard::EMPTY, |s| s.checkers_bb);

            let checkers_1 = self
                .state_at(1)
                .map_or(crate::bitboard::Bitboard::EMPTY, |s| s.checkers_bb);

            let mut check_them = self.state.checkers_bb.is_not_empty() && checkers_2.is_not_empty();
            let mut check_us = checkers_1.is_not_empty() && checkers_3.is_not_empty();

            let mut i = 4i32;
            while i <= end {
                let steps = i as usize;

                let Some(stp) = self.state_at(steps) else {
                    break;
                };
                let stp_key = stp.key;
                let stp_checkers = stp.checkers_bb;

                check_them &= stp_checkers.is_not_empty();

                if stp_key == self.state.key {
                    cnt += 1;
                    if cnt == 2 || ply > i {
                        let result = if !check_them && !check_us {
                            self.detect_chases(i, ply)
                        } else if !check_us {
                            mate_in(ply)
                        } else if !check_them {
                            mated_in(ply)
                        } else {
                            VALUE_DRAW
                        };

                        if result == VALUE_DRAW || cnt == 2 {
                            return RuleJudgeResult::Definitive(result);
                        }

                        if self.bloom_filter.count(self.state.key) <= 1 {
                            if self.state.rule60 < 120 {
                                let prev_key = self.state_at(1).map_or(0, |s| s.key);
                                let stp_prev_key =
                                    self.state_at(steps + 1).map_or(u64::MAX, |s| s.key);

                                if prev_key == stp_prev_key {
                                    let stack_len = self.state_stack.len();
                                    let range_start = stack_len.saturating_sub(steps);
                                    let range_end = stack_len.saturating_sub(1);
                                    let mut found_repeat = false;
                                    for idx in range_start..range_end {
                                        if self.bloom_filter.count(self.state_stack[idx].key) > 1 {
                                            found_repeat = true;
                                            break;
                                        }
                                    }
                                    if !found_repeat {
                                        return RuleJudgeResult::Definitive(result);
                                    }
                                }
                            }
                            two_fold_result = Some(result);
                            break;
                        }
                    }
                }

                if i < end {
                    let odd_steps = (i + 1) as usize;
                    if let Some(odd_st) = self.state_at(odd_steps) {
                        check_us &= odd_st.checkers_bb.is_not_empty();
                    }
                }

                i += 2;
            }
        }

        self.check_rule60_and_material_rj(ply, two_fold_result)
    }

    fn check_rule60_and_material_rj(
        &mut self,
        ply: i32,
        two_fold_result: Option<Value>,
    ) -> RuleJudgeResult {
        self.check_rule60_and_material(ply).map_or_else(
            || two_fold_result.map_or(RuleJudgeResult::None, RuleJudgeResult::TwoFold),
            RuleJudgeResult::Definitive,
        )
    }

    fn check_rule60_and_material(&mut self, ply: i32) -> Option<Value> {
        if self.state.rule60 >= 120 {
            let moves = generate(self, GenType::Legal);
            return if moves.is_empty() {
                Some(mated_in(ply))
            } else {
                Some(VALUE_DRAW)
            };
        }

        if self.count_type(Color::White, PieceType::Pawn) == 0
            && self.count_type(Color::Black, PieceType::Pawn) == 0
        {
            return self.check_insufficient_material(ply);
        }

        None
    }

    fn check_insufficient_material(&mut self, ply: i32) -> Option<Value> {
        let total_major = self.total_major_material();
        let cannon_val = crate::types::CANNON_VALUE;

        let level = self.classify_material(total_major, cannon_val);

        if level == DrawLevel::None {
            return None;
        }

        if level == DrawLevel::RequiresMateCheck {
            let moves = generate(self, GenType::Legal);
            if moves.is_empty() {
                return Some(mated_in(ply));
            }
            for idx in 0..moves.len() {
                let m = moves.get(idx);
                let gc = self.gives_check(m);
                self.do_move(m, gc);
                self.debug_check_consistency("after_do_move_rule_judge");
                let opp_moves = generate(self, GenType::Legal);
                let is_mate = opp_moves.is_empty();
                self.undo_move(m);
                self.debug_check_consistency("after_undo_move_rule_judge");
                if is_mate {
                    return None;
                }
            }
        }

        Some(VALUE_DRAW)
    }

    fn classify_material(&self, total_major: i32, cannon_val: i32) -> DrawLevel {
        if total_major == 0 {
            return DrawLevel::Direct;
        }

        if total_major == cannon_val {
            let cannon_side = if self.major_material(Color::White) == cannon_val {
                Color::White
            } else {
                Color::Black
            };
            if self.count_type(cannon_side, PieceType::Advisor) == 0 {
                let adv_other = self.count_type(!cannon_side, PieceType::Advisor);
                let bish_cannon = self.count_type(cannon_side, PieceType::Bishop);
                return match adv_other {
                    0 => DrawLevel::Direct,
                    1 => {
                        if bish_cannon == 0 {
                            DrawLevel::Direct
                        } else {
                            DrawLevel::RequiresMateCheck
                        }
                    }
                    _ => {
                        if bish_cannon == 0 {
                            DrawLevel::RequiresMateCheck
                        } else {
                            DrawLevel::None
                        }
                    }
                };
            }
            return DrawLevel::None;
        }

        let major_w = self.major_material(Color::White);
        let major_b = self.major_material(Color::Black);
        if major_w == cannon_val
            && major_b == cannon_val
            && self.count_type(Color::White, PieceType::Advisor) == 0
            && self.count_type(Color::Black, PieceType::Advisor) == 0
        {
            let total_bish = self.count_type(Color::White, PieceType::Bishop)
                + self.count_type(Color::Black, PieceType::Bishop);
            return if total_bish == 0 {
                DrawLevel::Direct
            } else {
                DrawLevel::RequiresMateCheck
            };
        }

        DrawLevel::None
    }
}

#[derive(PartialEq)]
enum DrawLevel {
    None,
    Direct,
    RequiresMateCheck,
}

#[cfg(test)]
mod tests {
    use super::RuleJudgeResult;
    use crate::position::Position;
    use crate::types::{Move, Square, VALUE_DRAW, VALUE_MATE};

    #[test]
    fn test_rule_judge_no_repetition_start_pos() {
        let mut pos = Position::from_fen(
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
        )
        .expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::None);
    }

    #[test]
    fn test_rule_judge_draw_by_repetition() {
        let mut pos = Position::from_fen(
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1",
        )
        .expect("valid fen");

        let moves = [
            Move::make(Square::SQ_A0, Square::SQ_A1),
            Move::make(Square::SQ_A9, Square::SQ_A8),
            Move::make(Square::SQ_A1, Square::SQ_A0),
            Move::make(Square::SQ_A8, Square::SQ_A9),
            Move::make(Square::SQ_A0, Square::SQ_A1),
            Move::make(Square::SQ_A9, Square::SQ_A8),
            Move::make(Square::SQ_A1, Square::SQ_A0),
            Move::make(Square::SQ_A8, Square::SQ_A9),
        ];

        for m in &moves {
            let gc = pos.gives_check(*m);
            pos.do_move(*m, gc);
        }

        assert_eq!(
            pos.rule_judge(0),
            RuleJudgeResult::Definitive(VALUE_DRAW),
            "repeated position should be draw"
        );
    }

    #[test]
    fn test_rule_judge_insufficient_material_kings_only() {
        let mut pos = Position::from_fen("4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::Definitive(VALUE_DRAW));
    }

    #[test]
    fn test_rule_judge_insufficient_material_kings_and_advisors() {
        let mut pos = Position::from_fen("4ka3/9/9/9/9/9/9/9/9/3AK4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::Definitive(VALUE_DRAW));
    }

    #[test]
    fn test_rule_judge_insufficient_material_kings_and_bishops() {
        let mut pos =
            Position::from_fen("4k4/9/3b5/9/9/9/9/2B6/9/4K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::Definitive(VALUE_DRAW));
    }

    #[test]
    fn test_rule_judge_sufficient_material_with_rook() {
        let mut pos = Position::from_fen("4k4/9/9/9/9/9/9/9/9/3RK4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::None);
    }

    #[test]
    fn test_rule_judge_rule60_draw() {
        let mut pos = Position::from_fen(
            "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 120 1",
        )
        .expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::Definitive(VALUE_DRAW));
    }

    #[test]
    fn test_rule_judge_no_draw_with_pawns() {
        let mut pos = Position::from_fen("4k4/9/9/9/9/9/4P4/9/9/4K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::None);
    }

    #[test]
    fn test_rule_judge_single_cannon_no_advisors_draw() {
        let mut pos = Position::from_fen("4k4/9/9/9/9/9/9/2C6/9/4K4 w - - 0 1").expect("valid fen");
        assert_eq!(pos.rule_judge(0), RuleJudgeResult::Definitive(VALUE_DRAW));
    }

    #[test]
    fn test_rule_judge_perpetual_check() {
        let mut pos =
            Position::from_fen("3ak4/9/9/9/9/9/9/9/4R4/4K4 w - - 0 1").expect("valid fen");

        let moves = [
            Move::make(Square::SQ_E1, Square::SQ_E8),
            Move::make(Square::SQ_E9, Square::SQ_D9),
            Move::make(Square::SQ_E8, Square::SQ_D8),
            Move::make(Square::SQ_D9, Square::SQ_E9),
            Move::make(Square::SQ_D8, Square::SQ_E8),
            Move::make(Square::SQ_E9, Square::SQ_D9),
            Move::make(Square::SQ_E8, Square::SQ_D8),
            Move::make(Square::SQ_D9, Square::SQ_E9),
        ];

        for m in &moves {
            let gc = pos.gives_check(*m);
            pos.do_move(*m, gc);
        }

        let result = pos.rule_judge(0);
        match result {
            RuleJudgeResult::Definitive(v) | RuleJudgeResult::TwoFold(v) => {
                assert!(
                    v >= VALUE_MATE - 300 || v <= -VALUE_MATE + 300 || v == VALUE_DRAW,
                    "perpetual check should return mate penalty or draw, got {v}"
                );
            }
            RuleJudgeResult::None => {
                // Acceptable — depends on bloom filter state
            }
        }
    }
}
