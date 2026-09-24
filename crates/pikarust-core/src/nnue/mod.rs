#![allow(unsafe_code)]

mod accumulator;
pub mod feature_transformer;
pub mod features;
pub mod layers;
mod model;
mod network;
pub mod simd;

pub use accumulator::{
    Accumulator, AccumulatorStack, DiffType, DirtyPiece, DirtyThreat, DirtyThreats,
};
pub use model::{L2_BIG, NnueError, NnueModel, WEIGHT_SCALE_BITS};
pub use network::{Network, make_layer_stack_bucket};

#[cfg(test)]
pub(crate) fn test_network() -> std::sync::Arc<Network> {
    static NETWORK: std::sync::OnceLock<std::sync::Arc<Network>> = std::sync::OnceLock::new();
    std::sync::Arc::clone(NETWORK.get_or_init(|| {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/pikafish.nnue");
        let model = NnueModel::load(&path).expect(
            "NNUE tests require the pinned model; run git lfs pull --include=models/pikafish.nnue",
        );
        std::sync::Arc::new(Network::new(model))
    }))
}
