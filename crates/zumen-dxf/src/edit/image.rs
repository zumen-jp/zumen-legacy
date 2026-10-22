//! 足す画像（docs/decisions/0017）。DXF と DWG の編集が、画像（IMAGE）の向き・画素の大きさと、画像の定義（IMAGEDEF）を
//! 載せる辞書 ACAD_IMAGE_DICT の項目の名前を決める。画像のファイルは読まず、path をそのまま IMAGEDEF に書く。

use zumen_core::edit::{EditError, ValueReader};

/// image の値のキー
pub const KEYS: [&str; 5] = ["path", "pixels", "position", "width", "rotation"];

/// 画像を足す辞書（名前つきオブジェクトの辞書の項目）
pub const DICTIONARY: &str = "ACAD_IMAGE_DICT";

/// 画像のクラス（DXF の名前、C++ の名前、DXF の 90 の値、図形か）。ezdxf の classes.py と同じ値
pub const CLASSES: [(&str, &str, i64, bool); 3] = [
    ("IMAGE", "AcDbRasterImage", 2175, true),
    ("IMAGEDEF", "AcDbRasterImageDef", 0, false),
    ("IMAGEDEF_REACTOR", "AcDbRasterImageDefReactor", 1, false),
];

/// クラスのアプリケーションの名前（DXF の 3）
pub const APPLICATION: &str = "ISM";

/// 足す画像
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInput {
    pub path: String,
    /// 画素の数（幅、高さ）
    pub pixels: [f64; 2],
    /// 左下の角
    pub position: [f64; 3],
    /// 画素 1 つ分の U（横）・V（縦）のベクトル。画素は正方形
    pub u_pixel: [f64; 3],
    pub v_pixel: [f64; 3],
}

/// 値を読む。width（図面の単位）の省略は 1 画素を 1 にする。rotation は度
pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<ImageInput, EditError> {
    let invalid = |message: &str| EditError::at(index, "invalid-value", message.to_string());
    let path = reader.required("path", reader.text("path")?)?;
    if path.trim().is_empty() {
        return Err(invalid("path は空にできません"));
    }
    let pixels = reader.required("pixels", reader.numbers("pixels", 2)?)?;
    if pixels.iter().any(|count| !(*count >= 1.0 && count.fract() == 0.0 && *count <= f64::from(u32::MAX))) {
        return Err(invalid("pixels は 1 以上の整数の [幅, 高さ] にしてください"));
    }
    let position = reader.required("position", reader.point("position")?)?;
    let width = reader.positive("width")?.unwrap_or(pixels[0]);
    let rotation = reader.number("rotation")?.unwrap_or(0.0).to_radians();
    let size = width / pixels[0];
    let (sin, cos) = rotation.sin_cos();
    Ok(ImageInput {
        path: path.to_string(),
        pixels: [pixels[0], pixels[1]],
        position,
        u_pixel: [size * cos, size * sin, 0.0],
        v_pixel: [-size * sin, size * cos, 0.0],
    })
}

/// ACAD_IMAGE_DICT の項目の名前。ファイルの名前から拡張子を除いたもので、使われていれば -2、-3… を付ける
pub fn entry_name(path: &str, used: impl Fn(&str) -> bool) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = match file.rfind('.') {
        Some(dot) if dot > 0 => &file[..dot],
        _ => file,
    };
    let stem: String = stem.chars().filter(|c| *c >= ' ' && !"<>/\\\":;?*|=`".contains(*c)).collect();
    let stem = if stem.trim().is_empty() { "image".to_string() } else { stem };
    if !used(&stem) {
        return stem;
    }
    (2..).map(|number| format!("{stem}-{number}")).find(|name| !used(name)).expect("an unused name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_entries_after_the_file() {
        assert_eq!(entry_name("C:\\maps\\site.png", |_| false), "site");
        assert_eq!(entry_name("maps/site.png", |name| name == "site"), "site-2");
        assert_eq!(entry_name("maps/.png", |_| false), ".png");
        assert_eq!(entry_name("a/", |_| false), "image");
    }
}
