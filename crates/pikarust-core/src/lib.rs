//! A library-first Xiangqi engine with legal move generation, NNUE evaluation,
//! and native search workers.
//!
//! Start with [`engine::Engine`] for application integration. The [`position`]
//! and [`types`] modules are also usable without loading a neural network or
//! starting search threads. Application transports and UCI process I/O live in
//! separate workspace crates.
//!
//! # Search with an explicit model
//!
//! Explicit model loading reports an error instead of falling back to material
//! evaluation. The network must match the model format supported by this build.
//!
//! ```no_run
//! use pikarust_core::engine::{Engine, SearchLimits};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut engine = Engine::with_nnue_file("models/pikafish.nnue")?;
//!     let limits = SearchLimits {
//!         depth: Some(8),
//!         ..SearchLimits::default()
//!     };
//!     let handle = engine.try_go(&limits)?;
//!     let result = handle.wait_result()?;
//!     println!("best move: {:?}, nodes: {}", result.best_move, result.nodes);
//!     Ok(())
//! }
//! ```
//!
//! [`engine::Engine::with_network`] shares immutable weights through
//! [`std::sync::Arc`], while each engine retains its own position and search
//! state. [`engine::Engine::without_nnue`] deliberately uses material evaluation
//! for model-independent applications or tests. [`engine::Engine::new`] retains
//! local model discovery and can fall back when no usable model is found.
//!
//! # Search lifecycle and scores
//!
//! [`engine::Engine::try_go`] validates search limits and returns a
//! [`engine::SearchHandle`]. Stop the handle to request cancellation, and consume
//! it with [`engine::SearchHandle::wait_result`] to receive a result or worker
//! error. Dropping an unfinished handle requests cancellation.
//!
//! Internal evaluation values, UCI centipawns, and mate scores have different
//! semantics; consult [`engine::SearchResult`] and [`engine::StaticEvaluation`]
//! before presenting them to users. Scalar and architecture-specific NNUE
//! backends implement the same numerical operations.

pub mod bitboard;
pub mod engine;
pub mod nnue;
pub mod position;
pub mod search;
pub mod types;
