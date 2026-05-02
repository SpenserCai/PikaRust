#![deny(unsafe_code)]

pub mod evaluate;
pub mod history;
mod history_update;
mod iterdeep;
pub mod movepick;
mod qsearch;
pub mod score;
#[allow(clippy::module_inception)]
pub mod search;
pub mod thread;
pub mod time;
pub mod tt;

pub use score::{to_cp, wdl};
pub use search::{PVLine, RootMove, Worker};
pub use thread::{SearchResult as ThreadSearchResult, ThreadPool};
pub use time::SearchLimits;
pub use tt::TranspositionTable;
