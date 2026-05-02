#![allow(unsafe_code)]

mod accumulator;
pub mod feature_transformer;
pub mod features;
pub mod layers;
mod model;
mod network;
#[cfg(test)]
mod regression_tests;
pub mod simd;

pub use accumulator::{Accumulator, AccumulatorStack, DiffType, DirtyPiece};
pub use model::NnueModel;
pub use network::Network;
