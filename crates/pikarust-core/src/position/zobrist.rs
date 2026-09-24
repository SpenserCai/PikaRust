// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use std::sync::OnceLock;

use crate::types::{Key, Piece, Square};

pub struct ZobristKeys {
    pub psq: [[Key; Square::NUM]; Piece::NUM],
    pub side: Key,
    pub no_pawns: Key,
}

static ZOBRIST: OnceLock<ZobristKeys> = OnceLock::new();

struct Prng {
    s: u64,
}

impl Prng {
    const fn new(seed: u64) -> Self {
        Self { s: seed }
    }

    const fn rand64(&mut self) -> u64 {
        self.s ^= self.s >> 12;
        self.s ^= self.s << 25;
        self.s ^= self.s >> 27;
        self.s.wrapping_mul(2_685_821_657_736_338_717)
    }
}

fn init_zobrist() -> ZobristKeys {
    let mut rng = Prng::new(1_070_372);
    let mut psq = [[0u64; Square::NUM]; Piece::NUM];

    let pieces = [
        Piece::W_ROOK,
        Piece::W_ADVISOR,
        Piece::W_CANNON,
        Piece::W_PAWN,
        Piece::W_KNIGHT,
        Piece::W_BISHOP,
        Piece::W_KING,
        Piece::B_ROOK,
        Piece::B_ADVISOR,
        Piece::B_CANNON,
        Piece::B_PAWN,
        Piece::B_KNIGHT,
        Piece::B_BISHOP,
        Piece::B_KING,
    ];

    for pc in pieces {
        for key in &mut psq[pc.index()] {
            *key = rng.rand64();
        }
    }

    let side = rng.rand64();
    let no_pawns = rng.rand64();

    ZobristKeys {
        psq,
        side,
        no_pawns,
    }
}

pub fn zobrist() -> &'static ZobristKeys {
    ZOBRIST.get_or_init(init_zobrist)
}
