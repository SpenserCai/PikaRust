#![deny(unsafe_code)]

mod core;
mod options;

pub use self::core::{Engine, EngineError, SearchLimits, SearchResult};
pub use options::{EngineOptions, OptionError, UciOption};
