#![deny(unsafe_code)]

mod chase;
mod do_move;
mod fen;
mod legality;
mod movegen;
#[allow(clippy::module_inception)]
mod position;
mod rule_judge;
mod state;
mod zobrist;

pub use fen::FenError;
pub use movegen::{GenType, MoveList, generate};
pub use position::Position;
pub use state::{BloomFilter, StateInfo};
pub use zobrist::zobrist;
