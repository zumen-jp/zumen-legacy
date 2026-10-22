//! ブロックの属性（docs/decisions/0017）。DXF と DWG の編集が、ブロックの属性の定義（ATTDEF）の値を読み、ブロック参照に
//! 書く属性（ATTRIB）の位置を、ブロックの基点と参照の位置・尺度・回転から求める（AutoCAD の INSERT と同じ置き方）。

use zumen_core::edit::{EditError, ValueReader};

/// attribute-definition の値のキー（共通の値と、文字の位置合わせ placement::KEYS のほか）
pub const DEFINITION_KEYS: [&str; 7] = ["tag", "prompt", "text", "position", "height", "rotation", "textStyle"];

/// 属性の定義の値
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub tag: String,
    pub prompt: String,
    /// 既定の値
    pub default: String,
    pub position: [f64; 3],
    /// 位置合わせの点（72・74 が 0 でないとき）
    pub align: Option<[f64; 3]>,
    pub height: f64,
    /// 度
    pub rotation: f64,
    pub width: f64,
    pub oblique: f64,
    pub generation: i64,
    pub horizontal: i64,
    pub vertical: i64,
    pub flags: i64,
    pub style: Option<String>,
}

/// タグに使えない文字（空白を含めない。AutoCAD の ATTDEF の規則）
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty() && !tag.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// attribute-definition の tag・prompt・text を読む
pub fn read_definition(reader: &ValueReader<'_>, index: usize) -> Result<(String, String, String), EditError> {
    let tag = reader.required("tag", reader.name("tag")?)?;
    if !valid_tag(tag) {
        return Err(EditError::at(index, "invalid-value", format!("属性のタグ {tag} は使えません（空白を含めない）")));
    }
    // AutoCAD はタグを大文字で持つ
    let tag = tag.to_uppercase();
    let prompt = reader.text("prompt")?.unwrap_or("").to_string();
    let default = reader.text("text")?.unwrap_or("").to_string();
    Ok((tag, prompt, default))
}

/// ブロックの中の点を、参照の座標にする（基点を引き、尺度を掛け、回し、参照の位置を足す）
pub fn transform(point: [f64; 3], base: [f64; 3], position: [f64; 3], scale: [f64; 3], rotation: f64) -> [f64; 3] {
    let local = [(point[0] - base[0]) * scale[0], (point[1] - base[1]) * scale[1], (point[2] - base[2]) * scale[2]];
    let (sin, cos) = rotation.to_radians().sin_cos();
    [
        position[0] + local[0] * cos - local[1] * sin,
        position[1] + local[0] * sin + local[1] * cos,
        position[2] + local[2],
    ]
}

/// 参照に書く属性
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// 属性の定義の並びでの位置（元の見た目の値を写すため）
    pub source: usize,
    pub definition: Definition,
    pub value: String,
    pub position: [f64; 3],
    pub align: Option<[f64; 3]>,
    pub height: f64,
    pub rotation: f64,
    pub width: f64,
    pub oblique: f64,
    pub generation: i64,
}

/// 参照の attributes（タグ → 値）を、ブロックの属性の定義ごとの属性にする。定義にないタグはエラー、値のない定義は
/// 既定の値。定数の属性（70 の 2）は ATTRIB を持たない
pub fn place(
    definitions: &[Definition],
    values: &std::collections::BTreeMap<String, String>,
    base: [f64; 3],
    position: [f64; 3],
    scale: [f64; 3],
    rotation: f64,
    index: usize,
) -> Result<Vec<Placed>, EditError> {
    if let Some(unknown) =
        values.keys().find(|tag| !definitions.iter().any(|definition| definition.tag.eq_ignore_ascii_case(tag)))
    {
        let tags: Vec<&str> = definitions.iter().map(|definition| definition.tag.as_str()).collect();
        return Err(EditError::at(
            index,
            "invalid-value",
            format!("ブロックに属性 {unknown} の定義がありません（あるのは {}）", tags.join("、")),
        ));
    }
    Ok(definitions
        .iter()
        .enumerate()
        .filter(|(_, definition)| definition.flags & 2 == 0)
        .map(|(source, definition)| {
            let value = values
                .iter()
                .find(|(tag, _)| tag.eq_ignore_ascii_case(&definition.tag))
                .map_or_else(|| definition.default.clone(), |(_, value)| value.clone());
            // DXF Reference「TEXT」40・41・50・51・71。INSERT の XY の尺度は文字の基線と縦軸にも掛かる。
            // 非等方尺度で傾く縦軸を、高さ・幅・斜体に分ける。反転は文字生成フラグの上下反転で持つ。
            let (sin, cos) = definition.rotation.to_radians().sin_cos();
            let baseline = [scale[0] * cos, scale[1] * sin];
            let up = [-scale[0] * sin, scale[1] * cos];
            let horizontal = baseline[0].hypot(baseline[1]);
            let vertical = scale[1].abs() * (scale[0].abs() / horizontal);
            let shear = (baseline[0] * up[0] + baseline[1] * up[1]) / horizontal;
            let mirrored =
                if (definition.generation & 2 != 0) != (definition.generation & 4 != 0) { -1.0 } else { 1.0 };
            let oblique = ((horizontal * definition.oblique.to_radians().tan() + mirrored * shear) / vertical)
                .atan()
                .to_degrees();
            Placed {
                source,
                definition: definition.clone(),
                value,
                position: transform(definition.position, base, position, scale, rotation),
                align: definition.align.map(|align| transform(align, base, position, scale, rotation)),
                height: definition.height * vertical,
                rotation: baseline[1].atan2(baseline[0]).to_degrees() + rotation,
                width: definition.width * horizontal / vertical,
                oblique,
                generation: definition.generation
                    ^ if scale[0].is_sign_negative() != scale[1].is_sign_negative() { 4 } else { 0 },
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_attributes_like_insert() {
        let point = transform([15.0, 0.0, 0.0], [5.0, 0.0, 0.0], [100.0, 100.0, 0.0], [2.0, 2.0, 1.0], 90.0);
        assert!((point[0] - 100.0).abs() < 1e-9 && (point[1] - 120.0).abs() < 1e-9);
    }

    /// 文字の基線と縦軸の先端が、INSERT で文字全体を変換した位置に一致する。
    #[test]
    fn transforms_glyph_axes_with_nonuniform_scales_and_reflections() {
        let mut definition = Definition {
            tag: "A".into(),
            prompt: String::new(),
            default: "text".into(),
            position: [0.0; 3],
            align: None,
            height: 3.0,
            rotation: 0.0,
            width: 0.8,
            oblique: 0.0,
            generation: 0,
            horizontal: 0,
            vertical: 0,
            flags: 0,
            style: None,
        };
        let axes = |height: f64, width: f64, oblique: f64, rotation: f64, generation: i64| {
            let mx = if generation & 2 != 0 { -1.0 } else { 1.0 };
            let my = if generation & 4 != 0 { -1.0 } else { 1.0 };
            let baseline = [height * width * mx, 0.0, 0.0];
            let top = [height * oblique.to_radians().tan() * mx, height * my, 0.0];
            [baseline, top].map(|point| transform(point, [0.0; 3], [0.0; 3], [1.0; 3], rotation))
        };
        for rotation in [0.0, 30.0, 90.0] {
            for oblique in [0.0, 15.0] {
                for generation in [0, 2, 4, 6] {
                    definition.rotation = rotation;
                    definition.oblique = oblique;
                    definition.generation = generation;
                    for scale in
                        [[2.0, 1.0, 1.0], [1.0, 2.0, 1.0], [-2.0, 1.0, 1.0], [2.0, -1.0, 1.0], [-2.0, -1.0, 1.0]]
                    {
                        let placed =
                            place(&[definition.clone()], &Default::default(), [0.0; 3], [0.0; 3], scale, 20.0, 0)
                                .unwrap()
                                .remove(0);
                        let expected = axes(definition.height, definition.width, oblique, rotation, generation)
                            .map(|point| transform(point, [0.0; 3], [0.0; 3], scale, 20.0));
                        let actual =
                            axes(placed.height, placed.width, placed.oblique, placed.rotation, placed.generation);
                        for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
                            assert!(
                                (actual - expected).abs() < 1e-9,
                                "rotation={rotation}, oblique={oblique}, generation={generation}, scale={scale:?}: {actual} != {expected}"
                            );
                        }
                    }
                }
            }
        }
        // 定数の属性を飛ばしても元の定義への対応はずれない。
        definition.flags = 2;
        let mut visible = definition.clone();
        visible.flags = 0;
        let placed = place(&[definition, visible], &Default::default(), [0.0; 3], [0.0; 3], [1.0; 3], 0.0, 0).unwrap();
        assert_eq!(placed[0].source, 1);
    }
}
