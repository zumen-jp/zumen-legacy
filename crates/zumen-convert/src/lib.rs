//! SXF と JWW の図面を、AutoCAD の図面のモデル（zumen-dxf の `DxfDocument`）に写す。写したモデルは
//! `zumen_dxf::write_dxf` で DXF に書ける（決定 0006・0007・0008）。逆に、モデルを SXF の図面に写し（`dxf_to_sxf`）、
//! `zumen_sxf::write_sfc` で SFC に書く。JWW の図面にも写し（`dxf_to_jww`）、`zumen_jww::write_jww` で JWW に書く。
//!
//! - モデル空間は実寸（mm）。SXF の部分図と JWW のレイヤグループの縮尺ごとに、用紙のレイアウトにビューポートを
//!   置き、元の用紙と同じ位置・縮尺で見せる。用紙にじかに描いた図形は紙空間に置く。
//! - 色・線種・線幅・文字・ブロック・寸法・ハッチング・画像を AutoCAD の図形にし、写せない・形を変えた内容は
//!   読み取りと同じ notices（種類ごとの件数）で返す。
#![forbid(unsafe_code)]

/// モデルを組み立てる部品。作図キット（zumen-draft、決定 0018）も使う
pub mod builder;
mod jww;
mod shapes;
/// 用紙とビューポート（決定 0008）。作図キットも使う
pub mod sheet;
mod sxf;
mod text;
mod to_jww;
mod to_sxf;

use std::collections::HashMap;
use zumen_core::Notice;
use zumen_dxf::DxfDocument;

pub use jww::jww_to_dxf;
pub use sxf::sxf_to_dxf;
pub use to_jww::{MappedJww, dxf_to_jww};
pub use to_sxf::{MappedSxf, dxf_to_sxf};

/// 写した図面のモデルと、写せなかった・形を変えて写した内容
#[derive(Debug, Clone, PartialEq)]
pub struct Mapped {
    pub document: DxfDocument,
    pub notices: Vec<Notice>,
    /// 図形のハンドル → 元の図形の id（SXF のインスタンス番号 #nn、JWW の図形の順番 #n）。ブロックの中の図形は
    /// ブロックの中での id、元の図形にあたらない図形（寸法の表示のブロックの中、全体のビューポート）は空
    pub sources: HashMap<String, String>,
}
