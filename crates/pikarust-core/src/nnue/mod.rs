#![allow(unsafe_code)]

mod accumulator;
pub mod feature_transformer;
pub mod features;
pub mod layers;
mod model;
mod network;
pub mod simd;

pub use accumulator::{Accumulator, AccumulatorStack, DiffType, DirtyPiece, DirtyThreat, DirtyThreats};
pub use model::{L2_BIG, NnueModel, WEIGHT_SCALE_BITS};
pub use network::{Network, make_layer_stack_bucket};
