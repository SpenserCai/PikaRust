use thiserror::Error;

use crate::types::{Color, File, Piece, PieceType, Rank, Square};

use crate::nnue::features::half_ka_v2_hm::BALANCE_ENCODING;

use super::position::Position;
use super::zobrist::zobrist;

#[derive(Debug, Error)]
pub enum FenError {
    #[error("invalid FEN: {0}")]
    Invalid(String),
    #[error("unsupported position: {0}")]
    Unsupported(String),
}

const PIECE_TO_CHAR: &[u8] = b" RACPNBK racpnbk";

const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1";

fn char_to_piece(ch: u8) -> Option<Piece> {
    PIECE_TO_CHAR.iter().position(|&c| c == ch).and_then(|idx| {
        if idx == 0 {
            None
        } else {
            Some(Piece::from_raw(idx as u8))
        }
    })
}

impl Piece {
    pub fn to_char(self) -> char {
        if self == Self::NONE {
            ' '
        } else {
            PIECE_TO_CHAR[self.index()] as char
        }
    }
}

impl Position {
    #[allow(clippy::too_many_lines)]
    pub fn from_fen(fen: &str) -> Result<Self, FenError> {
        let mut pos = Self::new();
        pos.mid_encoding_val = [BALANCE_ENCODING; Color::NUM];
        let mut chars = fen.bytes().peekable();

        let mut file = 0u8;
        let mut rank = 9u8;
        let mut num_pieces = 0u32;

        // 1. Piece placement
        loop {
            let Some(token) = chars.next() else {
                return Err(FenError::Invalid("unexpected end of stream".into()));
            };

            if token == b' ' {
                break;
            }

            if token.is_ascii_digit() {
                let diff = token - b'0';
                if diff < 1 {
                    return Err(FenError::Invalid(
                        "invalid number of squares to skip".into(),
                    ));
                }
                file += diff;
                if file > File::NUM as u8 {
                    return Err(FenError::Invalid("invalid file reached".into()));
                }
            } else if token == b'/' {
                if file != File::NUM as u8 {
                    return Err(FenError::Invalid(
                        "rank ended before reaching file I".into(),
                    ));
                }
                if rank == 0 {
                    return Err(FenError::Invalid("too many ranks".into()));
                }
                rank -= 1;
                file = 0;
            } else {
                if file >= File::NUM as u8 {
                    return Err(FenError::Invalid("invalid file reached".into()));
                }
                let Some(pc) = char_to_piece(token) else {
                    return Err(FenError::Invalid(format!(
                        "invalid piece: {}",
                        token as char
                    )));
                };
                num_pieces += 1;
                if num_pieces > 32 {
                    return Err(FenError::Invalid("more than 32 pieces on the board".into()));
                }
                let sq = Square::make(
                    File::try_from(file).map_err(|e| FenError::Invalid(e.to_string()))?,
                    Rank::try_from(rank).map_err(|e| FenError::Invalid(e.to_string()))?,
                );
                pos.put_piece(pc, sq);
                file += 1;
            }
        }

        if rank != 0 || file != File::NUM as u8 {
            return Err(FenError::Invalid(
                "board encoding ended at wrong position".into(),
            ));
        }

        // Validate piece counts
        let max_pieces: [u8; PieceType::PIECE_TYPE_NB] = [0, 2, 2, 2, 5, 2, 2, 1];
        for c in Color::ALL {
            for pt in PieceType::ALL {
                if pos.count_type(c, pt) > max_pieces[pt.index()] {
                    return Err(FenError::Unsupported(format!(
                        "{c} has more than {} {pt}s",
                        max_pieces[pt.index()]
                    )));
                }
            }
        }

        // 2. Active color
        let token = chars
            .next()
            .ok_or_else(|| FenError::Invalid("missing side to move".into()))?;
        pos.side_to_move = match token {
            b'w' => Color::White,
            b'b' => Color::Black,
            _ => {
                return Err(FenError::Invalid(format!(
                    "invalid side to move: {}",
                    token as char
                )));
            }
        };

        // Skip space after side-to-move
        skip_to_space(&mut chars);
        // Skip castling field (always "-" in xiangqi)
        skip_to_space(&mut chars);
        // Skip en passant field (always "-" in xiangqi)
        skip_to_space(&mut chars);

        // 3-4. Halfmove clock and fullmove number
        let rule60 = parse_int(&mut chars).unwrap_or(0);
        let fullmove = parse_int(&mut chars).unwrap_or(1);

        if !(0..=120).contains(&rule60) {
            return Err(FenError::Unsupported("rule60 counter out of range".into()));
        }

        pos.state.rule60 = rule60;
        pos.game_ply =
            (2 * (fullmove - 1).max(0) + i32::from(pos.side_to_move == Color::Black)) as u16;

        pos.set_state();

        Ok(pos)
    }

    pub fn start_pos() -> Result<Self, FenError> {
        Self::from_fen(START_FEN)
    }

    pub fn fen(&self) -> String {
        let mut s = String::with_capacity(80);

        for r in (0..10u8).rev() {
            let mut empty_cnt = 0u8;
            for f in 0..9u8 {
                let sq = Square::from_raw_unchecked(r * 9 + f);
                let pc = self.piece_on(sq);
                if pc == Piece::NONE {
                    empty_cnt += 1;
                } else {
                    if empty_cnt > 0 {
                        s.push((b'0' + empty_cnt) as char);
                        empty_cnt = 0;
                    }
                    s.push(pc.to_char());
                }
            }
            if empty_cnt > 0 {
                s.push((b'0' + empty_cnt) as char);
            }
            if r > 0 {
                s.push('/');
            }
        }

        s.push(' ');
        s.push(if self.side_to_move == Color::White {
            'w'
        } else {
            'b'
        });
        s.push_str(" - - ");
        s.push_str(&self.state.rule60.to_string());
        s.push(' ');
        let fullmove =
            1 + (i32::from(self.game_ply) - i32::from(self.side_to_move == Color::Black)) / 2;
        s.push_str(&fullmove.to_string());

        s
    }

    pub(crate) fn set_state(&mut self) {
        let z = zobrist();

        self.state.key = 0;
        self.state.minor_piece_key = 0;
        self.state.non_pawn_key = [0; Color::NUM];
        self.state.pawn_key = z.no_pawns;
        self.state.major_material = [0; Color::NUM];
        self.state.last_move = crate::types::Move::NONE;

        let mut bb = self.all_pieces();
        while bb.is_not_empty() {
            let sq = bb.pop_lsb();
            let pc = self.piece_on(sq);
            let pt = pc.piece_type();
            let c = pc.color();

            self.state.key ^= z.psq[pc.index()][sq.index()];

            if pt == PieceType::Pawn {
                self.state.pawn_key ^= z.psq[pc.index()][sq.index()];
            } else {
                self.state.non_pawn_key[c] ^= z.psq[pc.index()][sq.index()];

                if pt != PieceType::King && (pt as u8 & 1) != 0 {
                    self.state.major_material[c] += crate::types::PIECE_VALUE[pc];
                    if pt != PieceType::Rook {
                        self.state.minor_piece_key ^= z.psq[pc.index()][sq.index()];
                    }
                }
            }
        }

        if self.side_to_move == Color::Black {
            self.state.key ^= z.side;
        }

        self.state.checkers_bb = self.checkers_to(
            !self.side_to_move,
            self.king_square(self.side_to_move),
            self.all_pieces(),
        );
        self.set_check_info();
    }
}

fn skip_to_space(chars: &mut std::iter::Peekable<std::str::Bytes<'_>>) {
    for ch in chars.by_ref() {
        if ch == b' ' {
            return;
        }
    }
}

fn parse_int(chars: &mut std::iter::Peekable<std::str::Bytes<'_>>) -> Option<i32> {
    // Skip leading whitespace
    while chars.peek().is_some_and(|&c| c == b' ') {
        chars.next();
    }

    let mut result: i32 = 0;
    let mut found = false;
    let negative = chars.peek().is_some_and(|&c| c == b'-');
    if negative {
        chars.next();
    }

    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            found = true;
            result = result * 10 + i32::from(ch - b'0');
            chars.next();
        } else {
            break;
        }
    }

    if found {
        Some(if negative { -result } else { result })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_pos_fen_roundtrip() {
        let pos = Position::start_pos().expect("start_pos should parse");
        let fen = pos.fen();
        assert_eq!(fen, START_FEN);
    }

    #[test]
    fn test_start_pos_piece_counts() {
        let pos = Position::start_pos().expect("start_pos should parse");
        assert_eq!(pos.count_type(Color::White, PieceType::Rook), 2);
        assert_eq!(pos.count_type(Color::White, PieceType::Knight), 2);
        assert_eq!(pos.count_type(Color::White, PieceType::Bishop), 2);
        assert_eq!(pos.count_type(Color::White, PieceType::Advisor), 2);
        assert_eq!(pos.count_type(Color::White, PieceType::King), 1);
        assert_eq!(pos.count_type(Color::White, PieceType::Cannon), 2);
        assert_eq!(pos.count_type(Color::White, PieceType::Pawn), 5);
        assert_eq!(pos.count_type(Color::Black, PieceType::Rook), 2);
        assert_eq!(pos.count_type(Color::Black, PieceType::Pawn), 5);
    }

    #[test]
    fn test_start_pos_side_to_move() {
        let pos = Position::start_pos().expect("start_pos should parse");
        assert_eq!(pos.side_to_move(), Color::White);
    }

    #[test]
    fn test_start_pos_king_squares() {
        let pos = Position::start_pos().expect("start_pos should parse");
        assert_eq!(pos.king_square(Color::White), Square::SQ_E0);
        assert_eq!(pos.king_square(Color::Black), Square::SQ_E9);
    }

    #[test]
    fn test_start_pos_all_pieces() {
        let pos = Position::start_pos().expect("start_pos should parse");
        assert_eq!(pos.all_pieces().popcount(), 32);
    }

    #[test]
    fn test_invalid_fen_too_many_pieces() {
        let result = Position::from_fen("RRRRRRRRR/9/9/9/9/9/9/9/9/9 w - - 0 1");
        assert!(result.is_err());
    }

    #[test]
    fn test_fen_with_black_to_move() {
        let fen = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR b - - 0 1";
        let pos = Position::from_fen(fen).expect("should parse");
        assert_eq!(pos.side_to_move(), Color::Black);
    }

    #[test]
    fn test_fen_custom_position() {
        let fen = "4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1";
        let pos = Position::from_fen(fen).expect("should parse");
        assert_eq!(pos.all_pieces().popcount(), 2);
        assert_eq!(pos.king_square(Color::White), Square::SQ_E0);
        assert_eq!(pos.king_square(Color::Black), Square::SQ_E9);
    }
}
