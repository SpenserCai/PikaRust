use pikarust_core::nnue::{Network, NnueModel};

pub fn network() -> &'static Network {
    static NETWORK: std::sync::OnceLock<Network> = std::sync::OnceLock::new();
    NETWORK.get_or_init(|| {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models/pikafish.nnue");
        let model = NnueModel::load(&path).expect(
            "NNUE tests require the pinned model; run git lfs pull --include=models/pikafish.nnue",
        );
        Network::new(model)
    })
}
