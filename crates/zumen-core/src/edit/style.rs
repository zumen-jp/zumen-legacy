//! スタイルの操作（add-dimension-style・add-text-style・add-line-type。docs/decisions/0017・0026）の値の読み方。
//! DXF・DWG・JWW・SXF の編集が同じキーを同じ規則で読み、形式ごとに書く。線種の長さを、線から始まり線と空白が交互に
//! 並ぶ正の長さ（SXF のユーザ定義線種と JWW の SXF 対応線種のピッチ）にする規則もここにある。

use super::{EditError, EditValue, ValueReader};

/// add-dimension-style の値のキー（docs/decisions/0017）。from は写す寸法スタイル、textStyle は寸法値の文字スタイル
pub const DIMENSION_KEYS: [&str; 13] = [
    "from",
    "textStyle",
    "scale",
    "textHeight",
    "arrowSize",
    "extensionOffset",
    "extensionExtension",
    "textGap",
    "textAbove",
    "decimals",
    "suppressTrailingZeros",
    "tickSize",
    "linearFactor",
];

/// add-text-style の値のキー（docs/decisions/0017）
pub const TEXT_KEYS: [&str; 5] = ["font", "bigFont", "height", "widthFactor", "oblique"];

/// add-line-type の値のキー（docs/decisions/0017）
pub const LINE_TYPE_KEYS: [&str; 2] = ["description", "pattern"];

/// 線種の線・空白・点の長さの数の上限（DXF の LTYPE と同じ）
const MAX_PATTERN: usize = 12;

/// 線種の点（長さ 0 の線）を書く線の長さ（用紙の mm。zumen-convert の DXF → SXF・JWW と同じ）
pub const DOT: f64 = 0.25;

/// add-dimension-style の値。指定のないものは None（写す寸法スタイルの値のまま）。長さは scale を掛ける前の値
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DimensionStyleInput {
    /// 写す寸法スタイル
    pub from: Option<String>,
    /// 寸法値の文字スタイル（DIMTXSTY）
    pub text_style: Option<String>,
    /// 長さに掛ける尺度（DIMSCALE。0 は AutoCAD ではビューポートに合わせる）
    pub scale: Option<f64>,
    /// 寸法値の高さ（DIMTXT）
    pub text_height: Option<f64>,
    /// 矢印の長さ（DIMASZ）
    pub arrow_size: Option<f64>,
    /// 寸法補助線の起点からの離れ（DIMEXO）
    pub extension_offset: Option<f64>,
    /// 寸法補助線が寸法線を越える長さ（DIMEXE）
    pub extension_extension: Option<f64>,
    /// 寸法値と寸法線の間（DIMGAP）
    pub text_gap: Option<f64>,
    /// 斜線の大きさ（DIMTSZ。0 より大きければ矢印の代わり）
    pub tick_size: Option<f64>,
    /// 計測値に掛ける倍率（DIMLFAC）
    pub linear_factor: Option<f64>,
    /// 寸法値を寸法線の上に置く（DIMTAD 1。false は 0、寸法線の中）
    pub text_above: Option<bool>,
    /// 小数点以下の桁数（DIMDEC、0〜8）
    pub decimals: Option<u32>,
    /// 小数点以下の後ろの 0 を省く（DIMZIN 8）
    pub suppress_trailing_zeros: Option<bool>,
}

impl DimensionStyleInput {
    /// 値を読む。長さは 0 以上、linearFactor は 0 以外、decimals は 0〜8 の整数
    pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<Self, EditError> {
        reader.allow(&DIMENSION_KEYS, "寸法スタイル")?;
        let mut input = Self::default();
        let numbers: [(&str, &mut Option<f64>); 8] = [
            ("scale", &mut input.scale),
            ("textHeight", &mut input.text_height),
            ("arrowSize", &mut input.arrow_size),
            ("extensionOffset", &mut input.extension_offset),
            ("extensionExtension", &mut input.extension_extension),
            ("textGap", &mut input.text_gap),
            ("tickSize", &mut input.tick_size),
            ("linearFactor", &mut input.linear_factor),
        ];
        for (key, slot) in numbers {
            if let Some(value) = reader.number(key)? {
                let valid = if key == "linearFactor" { value != 0.0 } else { value >= 0.0 };
                if !valid {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        format!("{key} は 0 以上（linearFactor は 0 以外）の数にしてください"),
                    ));
                }
                *slot = Some(value);
            }
        }
        input.text_above = reader.bool("textAbove")?;
        if let Some(decimals) = reader.number("decimals")? {
            if !(0.0..=8.0).contains(&decimals) || decimals.fract() != 0.0 {
                return Err(EditError::at(index, "invalid-value", "decimals は 0〜8 の整数にしてください"));
            }
            input.decimals = Some(decimals as u32);
        }
        input.suppress_trailing_zeros = reader.bool("suppressTrailingZeros")?;
        input.from = reader.name("from")?.map(str::to_string);
        input.text_style = reader.name("textStyle")?.map(str::to_string);
        Ok(input)
    }
}

/// add-text-style の値
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyleInput {
    /// フォント（拡張子つきのファイルか、TrueType の書体の名前）。指定がなければ None
    pub font: Option<String>,
    /// SHX のビッグフォント（なければ空）
    pub big_font: String,
    /// 決まった文字の高さ（0 は決めない）
    pub height: f64,
    /// 文字の幅の倍率
    pub width_factor: f64,
    /// 斜体の角度（度）
    pub oblique: f64,
}

impl TextStyleInput {
    /// 値を読む。height は 0 以上、widthFactor は 0 より大きい数
    pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<Self, EditError> {
        reader.allow(&TEXT_KEYS, "文字スタイル")?;
        let font = reader.name("font")?.map(str::to_string);
        let big_font = reader.text("bigFont")?.unwrap_or("").to_string();
        let height = reader.number("height")?.unwrap_or(0.0);
        let width_factor = reader.positive("widthFactor")?.unwrap_or(1.0);
        let oblique = reader.number("oblique")?.unwrap_or(0.0);
        if height < 0.0 {
            return Err(EditError::at(index, "invalid-value", "height は 0 以上の数にしてください"));
        }
        Ok(Self { font, big_font, height, width_factor, oblique })
    }

    /// フォントのほかに値を持つか（ビッグフォント、0 でない高さ、1 でない幅、0 でない斜体。フォントだけを持つ形式は
    /// 書けない値として知らせる）
    pub fn has_more_than_font(&self) -> bool {
        !self.big_font.trim().is_empty() || self.height != 0.0 || self.width_factor != 1.0 || self.oblique != 0.0
    }
}

/// add-line-type の値
#[derive(Debug, Clone, PartialEq)]
pub struct LineTypeInput {
    /// 説明（なければ空）
    pub description: String,
    /// 線（正）・空白（負）・点（0）の長さ。実線は空
    pub pattern: Vec<f64>,
}

impl LineTypeInput {
    /// 値を読む。pattern は 12 個までの数の配列（1 つなら数でもよい）
    pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<Self, EditError> {
        reader.allow(&LINE_TYPE_KEYS, "線種")?;
        let description = reader.text("description")?.unwrap_or("").to_string();
        let pattern = match reader.value("pattern") {
            None => Vec::new(),
            Some(EditValue::Numbers(values)) if values.len() <= MAX_PATTERN => values.clone(),
            Some(EditValue::Number(value)) => vec![*value],
            Some(_) => {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("pattern は {MAX_PATTERN} 個までの数の配列にしてください"),
                ));
            }
        };
        Ok(Self { description, pattern })
    }
}

/// 線・空白・点の長さ（正は線、負は空白、0 は点。DXF の LTYPE・PAT と同じ並び）を、線から始まり線と空白が交互に並ぶ
/// 正の長さ（SXF のユーザ定義線種・JWW の SXF 対応線種のピッチ）にする。点は 0.25 mm の線にし、続く空白をその分だけ
/// 短くする（繰り返しの長さは変えない）。続く線・空白は 1 つにし、最初の空白は後ろに回し、最後の線は最初の線に
/// つなぐ。そのときに基点を線の向きにずらす長さも返す。空白のない模様（線だけ・空）は実線で、空を返す
pub fn pitches(dashes: &[f64]) -> (Vec<f64>, f64) {
    let mut items: Vec<(bool, f64)> = Vec::new();
    for (position, dash) in dashes.iter().enumerate() {
        let item = if *dash > 0.0 {
            (true, *dash)
        } else if *dash == 0.0 {
            (true, DOT)
        } else {
            (false, -dash)
        };
        // 点の後の空白は、点の長さの分だけ短くする（長さは変えず、繰り返しの長さを保つ）
        let item = match (item, dashes.get(position.wrapping_sub(1))) {
            ((false, gap), Some(previous)) if *previous == 0.0 && gap > DOT * 2.0 => (false, gap - DOT),
            (item, _) => item,
        };
        match items.last_mut() {
            Some(last) if last.0 == item.0 => last.1 += item.1,
            _ => items.push(item),
        }
    }
    let mut shift = 0.0;
    if items.len() > 1 && items[0].0 == items[items.len() - 1].0 {
        let (_, length) = items.pop().expect("2 つ以上");
        items[0].1 += length;
        shift -= length;
    }
    if items.first().is_some_and(|first| !first.0) {
        let gap = items.remove(0);
        shift += gap.1;
        items.push(gap);
    }
    if items.len() < 2 {
        return (Vec::new(), 0.0);
    }
    (items.into_iter().map(|(_, length)| length).collect(), shift)
}

/// 線種の模様（add-line-type の pattern）を、線から始まる線と空白の長さ（`pitches`）にし、max 個を超える分は線と空白の
/// 組ごとに切る。模様を書き換えたか（点を線にした、空白から始まる・続く線や空白をまとめた、切った、空白だけの模様を
/// 実線にした）も返す。空白のない模様は実線で、空を返す
pub fn line_type_pitches(pattern: &[f64], max: usize) -> (Vec<f64>, bool) {
    let (mut lengths, shift) = pitches(pattern);
    let alternating = pattern.iter().enumerate().all(|(position, length)| match position % 2 {
        0 => *length > 0.0,
        _ => *length < 0.0,
    });
    let mut changed = if lengths.is_empty() {
        pattern.iter().any(|length| *length < 0.0)
    } else {
        shift != 0.0
            || !alternating
            || lengths.len() != pattern.len()
            || lengths.iter().zip(pattern).any(|(length, dash)| *length != dash.abs())
    };
    let max = max - max % 2;
    if lengths.len() > max {
        lengths.truncate(max);
        changed = true;
    }
    (lengths, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditOperation, parse_operations};

    fn style_values(json: &str) -> crate::edit::EditValues {
        let operations = parse_operations(json).unwrap();
        let EditOperation::AddStyle { values, .. } = &operations[0] else { panic!() };
        values.clone()
    }

    #[test]
    fn reads_dimension_styles_with_the_checks_of_dxf() {
        let values = style_values(
            r#"[{"op": "add-dimension-style", "name": "DIM-50", "from": "DIM-100", "scale": 50, "textHeight": 3, "textAbove": false, "decimals": 2, "suppressTrailingZeros": true, "linearFactor": -1, "textStyle": "JP"}]"#,
        );
        let input = DimensionStyleInput::read(&ValueReader::new(&values, 0), 0).unwrap();
        assert_eq!(
            (input.from.as_deref(), input.scale, input.text_height, input.text_above, input.decimals),
            (Some("DIM-100"), Some(50.0), Some(3.0), Some(false), Some(2))
        );
        assert_eq!(
            (input.linear_factor, input.text_style.as_deref(), input.arrow_size),
            (Some(-1.0), Some("JP"), None)
        );
        for (json, code) in [
            (r#"[{"op": "add-dimension-style", "name": "D", "textGap": -1}]"#, "invalid-value"),
            (r#"[{"op": "add-dimension-style", "name": "D", "linearFactor": 0}]"#, "invalid-value"),
            (r#"[{"op": "add-dimension-style", "name": "D", "decimals": 1.5}]"#, "invalid-value"),
            (r#"[{"op": "add-dimension-style", "name": "D", "dimscale": 2}]"#, "unknown-key"),
        ] {
            let values = style_values(json);
            assert_eq!(DimensionStyleInput::read(&ValueReader::new(&values, 0), 0).unwrap_err().code, code, "{json}");
        }
    }

    #[test]
    fn reads_text_styles_and_line_types() {
        let values = style_values(r#"[{"op": "add-text-style", "name": "JP", "font": "MS Gothic", "widthFactor": 1}]"#);
        let input = TextStyleInput::read(&ValueReader::new(&values, 0), 0).unwrap();
        assert_eq!((input.font.as_deref(), input.has_more_than_font()), (Some("MS Gothic"), false));
        let values = style_values(r#"[{"op": "add-text-style", "name": "JP", "oblique": 15}]"#);
        assert!(TextStyleInput::read(&ValueReader::new(&values, 0), 0).unwrap().has_more_than_font());
        let values = style_values(r#"[{"op": "add-text-style", "name": "JP", "height": -1}]"#);
        assert_eq!(TextStyleInput::read(&ValueReader::new(&values, 0), 0).unwrap_err().code, "invalid-value");
        let values =
            style_values(r#"[{"op": "add-line-type", "name": "DASH", "description": "__ __", "pattern": [6, -3]}]"#);
        let input = LineTypeInput::read(&ValueReader::new(&values, 0), 0).unwrap();
        assert_eq!((input.description.as_str(), input.pattern), ("__ __", vec![6.0, -3.0]));
        let values = style_values(
            r#"[{"op": "add-line-type", "name": "DASH", "pattern": [1, -1, 1, -1, 1, -1, 1, -1, 1, -1, 1, -1, 1]}]"#,
        );
        assert_eq!(LineTypeInput::read(&ValueReader::new(&values, 0), 0).unwrap_err().code, "invalid-value");
    }

    #[test]
    fn turns_dashes_into_pitches_that_start_with_a_line() {
        assert_eq!(pitches(&[]), (Vec::new(), 0.0));
        assert_eq!(pitches(&[5.0]), (Vec::new(), 0.0));
        assert_eq!(pitches(&[2.0, -1.0]), (vec![2.0, 1.0], 0.0));
        // 空白から始まる模様は、空白を後ろに回して基点を進める
        assert_eq!(pitches(&[-1.0, 2.0]), (vec![2.0, 1.0], 1.0));
        // 点は 0.25 mm の線
        assert_eq!(pitches(&[0.0, -1.5]), (vec![0.25, 1.25], 0.0));
        // 最後の線は最初の線につなぐ
        assert_eq!(pitches(&[1.0, -1.0, 2.0]), (vec![3.0, 1.0], -2.0));
    }

    #[test]
    fn tells_whether_a_line_type_pattern_was_rewritten() {
        assert_eq!(line_type_pitches(&[6.0, -3.0], 8), (vec![6.0, 3.0], false));
        assert_eq!(line_type_pitches(&[], 8), (Vec::new(), false));
        // 点は 0.25 mm の線にし、続く空白をその分だけ短くする
        assert_eq!(line_type_pitches(&[6.0, -1.0, 0.0, -1.0], 8), (vec![6.0, 1.0, 0.25, 0.75], true));
        assert_eq!(line_type_pitches(&[-1.0, 2.0], 8), (vec![2.0, 1.0], true));
        // 空白だけの模様は実線にする（描き方が変わる）
        assert_eq!(line_type_pitches(&[-1.0], 8), (Vec::new(), true));
        // 10 個の長さは 8 個まで
        let long = [1.0, -1.0, 2.0, -1.0, 3.0, -1.0, 4.0, -1.0, 5.0, -1.0];
        assert_eq!(line_type_pitches(&long, 8), (vec![1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 4.0, 1.0], true));
        assert!(!line_type_pitches(&long, 10).1);
    }
}
