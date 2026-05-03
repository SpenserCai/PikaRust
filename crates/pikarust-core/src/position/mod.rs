#![deny(unsafe_code)]

mod chase;
mod do_move;
mod fen;
mod legality;
mod movegen;
pub mod perft;
#[allow(clippy::module_inception)]
mod position;
pub mod rule_judge;
mod state;
mod zobrist;

pub use fen::FenError;
pub use movegen::{GenType, MoveList, generate};
pub use perft::perft;
pub use position::Position;
pub use state::{BloomFilter, StateInfo};
pub use zobrist::zobrist;
