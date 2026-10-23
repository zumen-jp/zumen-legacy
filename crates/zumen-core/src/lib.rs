//! The format-neutral drawing model of zumen (`Scene`) and the helpers that format readers share.
//!
//! This is the Rust counterpart of `@zumenjs/core`. A `Scene` serializes to the same JSON as the
//! TypeScript `Scene` (`sceneToJson`), so the browser can draw what Rust reads and the two
//! implementations can be compared on the same drawings (`tests/fixtures`).
#![forbid(unsafe_code)]

pub mod checks;
pub mod compare;
pub mod edit;
pub mod extract;
pub mod geometry;
pub mod js;
pub mod limits;
pub mod markup;
mod measure;
mod notices;
mod read;
pub mod regions;
mod sort;
pub mod tables;
pub mod text;
pub mod title;
mod types;

pub use limits::{Limit, ReadLimits, limit_error, read_drawing_with_limits};
pub use measure::{Distance, MeasurePoint, measure_distance, to_real};
pub use notices::NoticeList;
pub use read::{DrawingReader, ReadError, ReadOptions, detect_format, read_drawing};
pub use types::*;
