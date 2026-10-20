//! The format-neutral drawing model of zumen (`Scene`) and the helpers that format readers share.
//!
//! This is the Rust counterpart of `@zumenjs/core`. A `Scene` serializes to the same JSON as the
//! TypeScript `Scene` (`sceneToJson`), so the browser can draw what Rust reads and the two
//! implementations can be compared on the same drawings (`tests/fixtures`).
#![forbid(unsafe_code)]

pub mod compare;
pub mod extract;
pub mod geometry;
pub mod js;
pub mod limits;
mod measure;
mod notices;
mod read;
mod sort;
pub mod text;
mod types;

pub use limits::{Limit, ReadLimits};
pub use measure::{Distance, MeasurePoint, measure_distance, to_real};
pub use notices::NoticeList;
pub use read::{DrawingReader, ReadError, ReadOptions, detect_format, read_drawing};
pub use types::*;
