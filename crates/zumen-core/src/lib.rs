//! zumen の形式に依存しない図面モデル（`Scene`）と、各形式の読み取りが共用する補助関数。
//!
//! TypeScript の `@zumenjs/core` に対応する。`Scene` は `sceneToJson` と同じ JSON になるため、
//! Rust が読んだ図面をブラウザで描き、同じテスト用図面（`tests/fixtures`）で両実装を比較できる。
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
