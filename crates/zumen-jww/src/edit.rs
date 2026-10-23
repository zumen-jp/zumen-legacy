//! JWW のその場の編集（docs/decisions/0012、docs/formats/jww.md「Editing」）。読んだファイル（`JwwFile`）の図形の
//! 一覧のレコードを変え・足し・消し、変えていないレコードは元のバイト列のまま書き戻す。版・ヘッダの設定・ブロック定義・
//! 同梱画像も元のまま。
//!
//! - id は表示用の図面の id（図形の一覧の何番目か、#5 など）。操作はすべて読んだときの図面の id で図形を選び、
//!   足した図形の id（`created`）は書いた図面での id。ブロックの中の図形（#5>#1）は定義を変えることになるので選べない。
//! - 座標は実寸（用紙の mm × 図形のレイヤグループの縮尺、用紙の中心が原点）。表示用の図面の property の raw と同じ。
//!   文字の大きさ・間隔は Jw_cad と同じく用紙の mm。
//! - レイヤは 16 のレイヤグループの 16 のレイヤ（「0-A」の番号か名前）。レイヤを足すと、名前も図形もないレイヤに名前を付ける。
//! - ブロック定義を足し（add-block）、container の block:<名前> で中に図形を足せる。寸法は寸法図形、ハッチングは
//!   ソリッドとハッチの線にする（docs/decisions/0025）。

mod dimension;
mod hatch;

use crate::create::write_jww;
use crate::file::{JwwBlockRecord, JwwFile, JwwRecord};
use crate::header::{JwwSettings, JwwSxfSettings};
use crate::reader::{
    JwwArc, JwwBase, JwwBlock, JwwDocument, JwwEntity, JwwLine, JwwPoint, JwwSolid, JwwText, colorref, is_jww,
};
use std::collections::{BTreeSet, HashMap};
use std::f64::consts::TAU;
use zumen_core::edit::dimension::{DimensionGeometry, DimensionInput};
use zumen_core::edit::hatch::PatternLine;
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValue, EditValues, Edited, ValueReader,
};
use zumen_core::text::is_half_width;
use zumen_core::{NoticeLevel, NoticeList};

/// 足せる図形の種類（JWW）
const ADDABLE: [&str; 10] =
    ["line", "arc", "circle", "ellipse", "ellipse-arc", "point", "text", "dimension", "hatch", BLOCK_REFERENCE];

/// 版 4.10 以降のブロック名の後ろの、複合図形の種別の印（jwdatafmt.txt「ブロック定義データ」）
const SFIGORG: &str = "@@SfigorgFlag@@";

/// ハッチの線・点の属性フラグ（jwdatafmt.txt「属性フラグ」: 線・実点の 0x0020 はハッチ）
const HATCH_FLAG: u16 = 0x0020;

/// 入れ子のブロックを辿る深さの上限
const MAX_BLOCK_DEPTH: usize = 32;

/// レイヤの値のキー。color・lineType・lineWidth・frozen・plot は JWW のレイヤにないので、書かずに知らせる
const LAYER_KEYS: [&str; 9] =
    ["group", "visible", "locked", "scale", "color", "lineType", "lineWidth", "frozen", "plot"];

/// JWW のレイヤにない値
const LAYER_IGNORED: [&str; 5] = ["color", "lineType", "lineWidth", "frozen", "plot"];

/// 新しい文字のフォント（Jw_cad の既定）
pub(crate) const DEFAULT_FONT: &str = "ＭＳ ゴシック";

/// 新しい図形の線色（線色 2）
const DEFAULT_PEN_COLOR: u16 = 2;

/// 新しい空の図面（版 700、A3、すべてのレイヤグループが 1/1、長さは mm）。`zumen::create` が操作を当てる元
pub fn new_jww() -> Vec<u8> {
    write_jww(&JwwDocument::new()).bytes
}

/// JWW を編集する。すべての操作を確かめてから書き、1 つでも通らなければエラーを返して何も書かない
pub fn edit_jww(bytes: &[u8], operations: &[EditOperation]) -> Result<Edited, EditError> {
    if !is_jww(bytes) {
        return Err(EditError::new("unreadable", "JWW として読めません"));
    }
    let file = JwwFile::read(bytes).map_err(|error| {
        EditError::new("unreadable", format!("JWW を終わりまで読めないため、編集できません（{}）", error.message()))
    })?;
    let mut editor = Editor::new(file);
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            EditOperation::Add { kind, container, values } => editor.add(index, kind, container.as_deref(), values)?,
            EditOperation::Update { id, set } => editor.update(index, id, set)?,
            EditOperation::Delete { id } => editor.delete(index, id)?,
            EditOperation::AddLayer { name, values } => {
                let (group, layer) = editor.add_layer(index, name, values)?;
                editor.created.push(Created::Layer(scene_layer_name(group, layer, name)));
            }
            EditOperation::UpdateLayer { name, set } => editor.update_layer(index, name, set)?,
            EditOperation::AddBlock { name, values } => editor.add_block(index, name, values)?,
            EditOperation::AddLayout { .. } | EditOperation::UpdateLayout { .. } => {
                return Err(EditError::at(index, "unsupported-kind", "JWW の編集ではレイアウトを扱えません"));
            }
            EditOperation::AddStyle { kind, .. } => {
                return Err(EditError::at(
                    index,
                    "unsupported-kind",
                    format!("JWW の編集では {} をまだ使えません", kind.op()),
                ));
            }
        }
    }
    Ok(editor.finish())
}

/// 図形の一覧の 1 件と、読んだときの番号（足したものは None）
struct Slot {
    record: JwwRecord,
    origin: Option<usize>,
    deleted: bool,
}

/// 足したものの id。図形の id は書いた図面での位置で決める
enum Created {
    Layer(String),
    Entity(usize),
    /// ブロック定義の名前
    Block(String),
    /// ブロック定義の中の図形（定義の名前と、定義の中の何番目か）
    Member(String, usize),
}

/// 図形を足す先
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    /// 図形の一覧（container の group: で選んだレイヤグループ）
    List(Option<usize>),
    /// ブロック定義（file.blocks の添字）
    Block(usize),
}

struct Editor {
    file: JwwFile,
    slots: Vec<Slot>,
    created: Vec<Created>,
    notices: NoticeList,
    /// この編集で使った SXF 対応色の番号（線色 100〜356）
    allocated: BTreeSet<u16>,
    /// この編集で足したブロック定義の基点（定義の通し番号から、実寸の座標）
    block_bases: HashMap<u32, [f64; 2]>,
}

/// 「0-A」のようなレイヤの番号（グループとレイヤの 16 進の 1 桁）と、その後ろ
fn layer_code(text: &str) -> Option<(usize, usize)> {
    let text = text.trim();
    let mut chars = text.chars();
    let group = chars.next()?.to_digit(16)?;
    if chars.next()? != '-' {
        return None;
    }
    let layer = chars.next()?.to_digit(16)?;
    match chars.next() {
        None => Some((group as usize, layer as usize)),
        Some(c) if c.is_whitespace() => Some((group as usize, layer as usize)),
        Some(_) => None,
    }
}

/// 表示用の図面のレイヤの名前（「0-A 通り芯」。名前がなければ番号）
fn scene_layer_name(group: usize, layer: usize, name: &str) -> String {
    let code = format!("{group:X}-{layer:X}");
    let name = name.trim();
    if name.is_empty() || name.to_uppercase() == code { code } else { format!("{code} {name}") }
}

/// 名前に使えるか（空でなく、制御文字がなく、レイヤの番号と紛れない）
fn valid_layer_name(name: &str) -> bool {
    !name.trim().is_empty() && !name.chars().any(char::is_control) && layer_code(name).is_none()
}

/// 円弧の始角から終角まで反時計回りに回る角度（0 より大きく 360 以下、度）
fn ccw_sweep(start: f64, end: f64) -> f64 {
    let sweep = (end - start).rem_euclid(360.0);
    if sweep == 0.0 { 360.0 } else { sweep }
}

/// 媒介変数の角度の始まりから終わりまで反時計回りに回る角度（0 より大きく 2π 以下）
fn ccw_parameter(start: f64, end: f64) -> f64 {
    let sweep = (end - start).rem_euclid(TAU);
    if sweep == 0.0 { TAU } else { sweep }
}

/// 図形の種類の名前（表示用の図面の kind と同じ）
fn kind_of(entity: &JwwEntity) -> &'static str {
    match entity {
        JwwEntity::Line(_) => "line",
        JwwEntity::Arc(arc) => {
            let ellipse = arc.flatness != 1.0 && (arc.flatness - 1.0).abs() > 1e-9;
            match (ellipse, arc.full) {
                (true, true) => "ellipse",
                (true, false) => "ellipse-arc",
                (false, true) => "circle",
                (false, false) => "arc",
            }
        }
        JwwEntity::Point(_) => "point",
        JwwEntity::Text(_) => "text",
        JwwEntity::Solid(_) | JwwEntity::CircleSolid(_) => "solid",
        JwwEntity::Block(_) => BLOCK_REFERENCE,
        JwwEntity::Dimension(_) => "dimension",
    }
}

/// 図形の属性（寸法は線・文字・補助線・点の属性も）
fn bases(entity: &JwwEntity) -> Vec<&JwwBase> {
    match entity {
        JwwEntity::Dimension(e) => {
            let mut bases = vec![&e.base, &e.line.base, &e.text.base];
            bases.extend(e.lines.iter().map(|line| &line.base));
            bases.extend(e.points.iter().map(|point| &point.base));
            bases
        }
        other => vec![other.base()],
    }
}

/// 図形の属性（変える）
fn bases_mut(entity: &mut JwwEntity) -> Vec<&mut JwwBase> {
    match entity {
        JwwEntity::Line(e) => vec![&mut e.base],
        JwwEntity::Arc(e) => vec![&mut e.base],
        JwwEntity::Point(e) => vec![&mut e.base],
        JwwEntity::Text(e) => vec![&mut e.base],
        JwwEntity::Solid(e) => vec![&mut e.base],
        JwwEntity::CircleSolid(e) => vec![&mut e.base],
        JwwEntity::Block(e) => vec![&mut e.base],
        JwwEntity::Dimension(e) => {
            let mut bases = vec![&mut e.base, &mut e.line.base, &mut e.text.base];
            bases.extend(e.lines.iter_mut().map(|line| &mut line.base));
            bases.extend(e.points.iter_mut().map(|point| &mut point.base));
            bases
        }
    }
}

/// 図形が使うレイヤ（グループ、レイヤ）
fn layers_of(entity: &JwwEntity, out: &mut BTreeSet<(usize, usize)>) {
    for base in bases(entity) {
        out.insert((usize::from(base.layer_group & 0x0f), usize::from(base.layer & 0x0f)));
    }
}

/// 用紙の座標と長さに倍率を掛ける（縮尺の違うレイヤグループに移して実寸を保つ）。文字の大きさと、文字の始点から
/// 終点までの用紙の長さは変えない
fn scale_entity(entity: &mut JwwEntity, factor: f64) {
    let text = |text: &mut JwwText| {
        let (dx, dy) = (text.x2 - text.x1, text.y2 - text.y1);
        text.x1 *= factor;
        text.y1 *= factor;
        text.x2 = text.x1 + dx;
        text.y2 = text.y1 + dy;
    };
    let line = |line: &mut JwwLine| {
        line.x1 *= factor;
        line.y1 *= factor;
        line.x2 *= factor;
        line.y2 *= factor;
    };
    match entity {
        JwwEntity::Line(e) => line(e),
        JwwEntity::Arc(e) => {
            e.cx *= factor;
            e.cy *= factor;
            e.radius *= factor;
        }
        JwwEntity::Point(e) => {
            e.x *= factor;
            e.y *= factor;
        }
        JwwEntity::Text(e) => text(e),
        JwwEntity::Solid(e) => e.points.iter_mut().for_each(|value| *value *= factor),
        JwwEntity::CircleSolid(e) => {
            e.cx *= factor;
            e.cy *= factor;
            e.radius *= factor;
            // 円環ソリッド（線種 105・106）は内側の円の半径を持つ
            if matches!(e.base.pen_style, 105 | 106) {
                e.mode *= factor;
            }
        }
        JwwEntity::Block(e) => {
            e.x *= factor;
            e.y *= factor;
            e.scale_x *= factor;
            e.scale_y *= factor;
        }
        JwwEntity::Dimension(e) => {
            line(&mut e.line);
            text(&mut e.text);
            e.lines.iter_mut().for_each(line);
            e.points.iter_mut().for_each(|point| {
                point.x *= factor;
                point.y *= factor;
            });
        }
    }
}

/// 文字の終点。Jw_cad と同じく、全角の文字は幅、半角の文字は幅の半分、文字の間に間隔を足した長さ
fn text_end(text: &mut JwwText) {
    let count = text.content.chars().count();
    let units: f64 = text.content.chars().map(|c| if is_half_width(c) { 0.5 } else { 1.0 }).sum();
    let length = units * text.size_x + count.saturating_sub(1) as f64 * text.spacing;
    let angle = text.angle.to_radians();
    text.x2 = text.x1 + length * angle.cos();
    text.y2 = text.y1 + length * angle.sin();
}

/// 図形の共通の値の指定
#[derive(Debug, Clone, Copy)]
struct Common {
    layer: Option<(usize, usize)>,
    color: Option<EditColor>,
    line_type: Option<u8>,
    line_width: Option<EditLineWidth>,
}

impl Editor {
    fn new(mut file: JwwFile) -> Self {
        let slots = std::mem::take(&mut file.entities)
            .into_iter()
            .enumerate()
            .map(|(index, record)| Slot { record, origin: Some(index + 1), deleted: false })
            .collect();
        Self {
            file,
            slots,
            created: Vec::new(),
            notices: NoticeList::new(),
            allocated: BTreeSet::new(),
            block_bases: HashMap::new(),
        }
    }

    /// 書く。足した図形の id は、書いた図面の中の位置
    fn finish(mut self) -> Edited {
        let mut positions = vec![0usize; self.slots.len()];
        let mut count = 0;
        for (index, slot) in self.slots.iter().enumerate() {
            if !slot.deleted {
                count += 1;
                positions[index] = count;
            }
        }
        let created = self
            .created
            .iter()
            .map(|created| match created {
                Created::Layer(name) | Created::Block(name) => name.clone(),
                Created::Entity(slot) => format!("#{}", positions[*slot]),
                Created::Member(block, position) => format!("block:{block}>#{position}"),
            })
            .collect();
        self.file.entities = self.slots.into_iter().filter(|slot| !slot.deleted).map(|slot| slot.record).collect();
        let written = self.file.write();
        for notice in &written.notices {
            let code = format!("edit-{}", notice.code);
            self.notices.add_count(notice.level, &code, notice.message.clone(), notice.count);
        }
        Edited { bytes: written.bytes, created, notices: self.notices.into_vec() }
    }

    fn settings(&self) -> Option<&JwwSettings> {
        self.file.header.settings.as_ref()
    }

    fn settings_mut(&mut self, index: usize) -> Result<&mut JwwSettings, EditError> {
        self.file.header.settings.as_mut().ok_or_else(|| {
            EditError::at(
                index,
                "invalid-value",
                "この JWW のヘッダはレイヤの名前を持たない（版 300 より前など）ため、レイヤを変えられません",
            )
        })
    }

    fn set_layer_name(&mut self, index: usize, group: usize, layer: usize, name: &str) -> Result<(), EditError> {
        let slot =
            self.settings_mut(index)?.layer_names.get_mut(group * 16 + layer).ok_or_else(|| {
                EditError::at(index, "invalid-value", "この JWW のヘッダはレイヤの名前の表を持ちません")
            })?;
        *slot = name.trim().to_string();
        Ok(())
    }

    fn layer_name(&self, group: usize, layer: usize) -> &str {
        self.settings().and_then(|settings| settings.layer_names.get(group * 16 + layer)).map_or("", String::as_str)
    }

    fn group_name(&self, group: usize) -> String {
        match self.settings().and_then(|settings| settings.group_names.get(group)).map(|name| name.trim()) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => format!("グループ {group:X}"),
        }
    }

    /// レイヤグループの縮尺の分母（0 以下・読めないものは 1。表示用の読み取りと同じ）
    fn scale(&self, group: usize) -> f64 {
        match self.file.header.groups.get(group) {
            Some(state) if state.scale > 0.0 => state.scale,
            _ => 1.0,
        }
    }

    /// 図形が使うレイヤ（消したものは除く。ブロック定義の中も）
    fn used_layers(&self) -> BTreeSet<(usize, usize)> {
        let mut used = BTreeSet::new();
        for slot in self.slots.iter().filter(|slot| !slot.deleted) {
            layers_of(&slot.record.entity, &mut used);
        }
        for block in &self.file.blocks {
            for record in &block.entities {
                layers_of(&record.entity, &mut used);
            }
        }
        used
    }

    /// 書込みレイヤグループ
    fn write_group(&self) -> usize {
        (self.file.header.write_group as usize).min(15)
    }

    /// グループの書込みレイヤ
    fn write_layer(&self, group: usize) -> usize {
        self.file.header.groups.get(group).map_or(0, |state| (state.write_layer as usize).min(15))
    }

    // ---- 名前の確かめ ----

    /// レイヤグループ。0〜15 の数、16 進の 1 桁（0〜F）か、レイヤグループの名前
    fn group(&self, index: usize, value: &EditValue) -> Result<usize, EditError> {
        let invalid = || {
            EditError::at(
                index,
                "invalid-value",
                "レイヤグループは 0〜15 の数、0〜F の番号か、レイヤグループの名前で指定してください",
            )
        };
        match value {
            EditValue::Number(number) if number.fract() == 0.0 && (0.0..16.0).contains(number) => Ok(*number as usize),
            EditValue::Text(text) => {
                let text = text.trim();
                let mut chars = text.chars();
                if let (Some(digit), None) = (chars.next().and_then(|c| c.to_digit(16)), chars.next()) {
                    return Ok(digit as usize);
                }
                let names = self.settings().map(|settings| settings.group_names.as_slice()).unwrap_or(&[]);
                let found: Vec<usize> =
                    names.iter().enumerate().filter(|(_, name)| name.trim() == text).map(|(group, _)| group).collect();
                match found[..] {
                    [group] => Ok(group),
                    [] => Err(EditError::at(index, "not-found", format!("レイヤグループ {text} は図面にありません"))),
                    _ => Err(EditError::at(
                        index,
                        "invalid-value",
                        format!("レイヤグループ {text} が 2 つ以上あります。0〜F の番号で指定してください"),
                    )),
                }
            }
            _ => Err(invalid()),
        }
    }

    /// 名前か番号のレイヤ。なければ None
    fn find_layer(&self, index: usize, value: &EditValue) -> Result<Option<(usize, usize)>, EditError> {
        match value {
            // 表示用の図面の layer の raw（グループ × 16 + レイヤ + 1）
            EditValue::Number(number) if number.fract() == 0.0 && (1.0..=256.0).contains(number) => {
                let id = *number as usize - 1;
                Ok(Some((id / 16, id % 16)))
            }
            EditValue::Text(text) => {
                if let Some(code) = layer_code(text) {
                    return Ok(Some(code));
                }
                let name = text.trim();
                let found: Vec<(usize, usize)> = (0..256)
                    .map(|id| (id / 16, id % 16))
                    .filter(|&(group, layer)| self.layer_name(group, layer).trim() == name)
                    .collect();
                match found[..] {
                    [] => Ok(None),
                    [layer] => Ok(Some(layer)),
                    _ => {
                        let codes: Vec<String> = found.iter().map(|(g, l)| format!("{g:X}-{l:X}")).collect();
                        Err(EditError::at(
                            index,
                            "invalid-value",
                            format!(
                                "レイヤ {name} が 2 つ以上あります（{}）。「0-A」のような番号で指定してください",
                                codes.join("、")
                            ),
                        ))
                    }
                }
            }
            _ => Err(EditError::at(
                index,
                "invalid-value",
                "レイヤは名前か「0-A」のような番号（グループとレイヤの 16 進）で指定してください",
            )),
        }
    }

    /// 図形のレイヤ。なければ、名前のないレイヤに名前を付けて足し、知らせる
    fn layer(&mut self, index: usize, value: &EditValue, group: Option<usize>) -> Result<(usize, usize), EditError> {
        if let Some(found) = self.find_layer(index, value)? {
            return Ok(found);
        }
        let EditValue::Text(name) = value else {
            return Err(EditError::at(index, "invalid-value", "レイヤの名前がありません"));
        };
        let mut values = EditValues::new();
        if let Some(group) = group {
            values.insert("group".to_string(), EditValue::Number(group as f64));
        }
        let layer = self.add_layer(index, name, &values)?;
        self.notices.add(
            NoticeLevel::Info,
            "edit-added-layer",
            "図面になかったレイヤを、名前のないレイヤに名前を付けて足しました",
        );
        Ok(layer)
    }

    /// 足す先。省略・model は書込みレイヤグループ、group:<番号か名前> はそのレイヤグループ、block:<名前> はその
    /// ブロック定義
    fn container(&self, index: usize, container: Option<&str>) -> Result<Target, EditError> {
        let container = container.map(str::trim).unwrap_or("");
        if container.is_empty() || container.eq_ignore_ascii_case("model") {
            return Ok(Target::List(None));
        }
        if let Some(group) = container.strip_prefix("group:") {
            return self.group(index, &EditValue::Text(group.to_string())).map(|group| Target::List(Some(group)));
        }
        if let Some(name) = container.strip_prefix("block:") {
            return match self.find_blocks(name)[..] {
                [block] => Ok(Target::Block(block)),
                [] => Err(EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))),
                _ => Err(EditError::at(index, "invalid-value", format!("ブロック {name} が 2 つ以上あります"))),
            };
        }
        Err(EditError::at(
            index,
            "invalid-value",
            format!(
                "container の {container} は使えません（JWW は group:<レイヤグループの番号か名前> と block:<ブロックの名前>）"
            ),
        ))
    }

    fn common(&mut self, index: usize, reader: &ValueReader<'_>, group: Option<usize>) -> Result<Common, EditError> {
        let layer = match reader.value("layer") {
            Some(value) => Some(self.layer(index, value, group)?),
            None => None,
        };
        let line_type = match reader.value("lineType") {
            Some(value) => Some(self.line_type(index, value)?),
            None => None,
        };
        Ok(Common { layer, color: reader.color("color")?, line_type, line_width: reader.line_width("lineWidth")? })
    }

    /// 線種の番号。1〜9（11〜15 はランダム線、16〜19 は倍長線種）、SXF 対応線種 30〜62（版 420 以降。
    /// 名前でも指定できる）
    fn line_type(&self, index: usize, value: &EditValue) -> Result<u8, EditError> {
        let sxf = self.file.version >= 420;
        let valid = |number: f64| {
            number.fract() == 0.0
                && ((1.0..=9.0).contains(&number)
                    || (11.0..=19.0).contains(&number)
                    || sxf && (30.0..=62.0).contains(&number))
        };
        let number = match value {
            EditValue::Number(number) => Some(*number),
            EditValue::Text(text) => match text.trim().parse::<f64>() {
                Ok(number) => Some(number),
                Err(_) => {
                    // SXF 対応線種の名前（dashed など）
                    let names = self.settings().and_then(|settings| settings.sxf.as_ref());
                    let found = names.and_then(|sxf| {
                        sxf.user_line_types.iter().position(|line_type| {
                            !line_type.name.is_empty() && line_type.name.eq_ignore_ascii_case(text.trim())
                        })
                    });
                    match found {
                        Some(position) => Some(30.0 + position as f64),
                        None => {
                            return Err(EditError::at(
                                index,
                                "missing-definition",
                                format!("線種 {text} は図面にありません"),
                            ));
                        }
                    }
                }
            },
            _ => None,
        };
        match number {
            Some(number) if valid(number) => Ok(number as u8),
            _ => Err(EditError::at(
                index,
                "invalid-value",
                if sxf {
                    "JWW の線種は 1〜9、11〜19 と SXF 対応線種 30〜62 の番号か、SXF 対応線種の名前です"
                } else {
                    "版 420 より前の JWW の線種は 1〜9 と 11〜19 の番号です"
                },
            )),
        }
    }

    // ---- 値の書き方 ----

    /// 線色の番号。1〜9、SXF 対応色 100〜356（版 420 以降）。#rrggbb は同じ色の線色、なければ SXF 対応色の
    /// ユーザー定義の色（117〜355）を足し、どちらもなければ最も近い線色にして知らせる
    fn pen_color(&mut self, index: usize, color: EditColor) -> Result<u16, EditError> {
        match color {
            EditColor::Index(number @ 1..=9) => Ok(number as u16),
            EditColor::Index(number @ 100..=356) if self.file.version >= 420 => Ok(number as u16),
            EditColor::Index(number) => Err(EditError::at(
                index,
                "invalid-value",
                format!("JWW の線色は 1〜9 と SXF 対応色 100〜356（版 420 以降）の番号です: {number}"),
            )),
            EditColor::ByLayer | EditColor::ByBlock => Err(EditError::at(
                index,
                "invalid-value",
                "JWW の線色にレイヤ・ブロックの色（bylayer・byblock）はありません",
            )),
            EditColor::Rgb(rgb) => Ok(self.rgb_pen(rgb)),
        }
    }

    fn rgb_pen(&mut self, rgb: u32) -> u16 {
        let colorref = ((rgb >> 16) & 0xff) | (rgb & 0xff00) | ((rgb & 0xff) << 16);
        let pens: Vec<u32> = self
            .settings()
            .and_then(|settings| settings.display.as_ref())
            .map(|display| display.pens.iter().map(|pen| pen.color & 0xff_ffff).collect())
            .unwrap_or_default();
        if let Some(pen) = (1..pens.len().min(10)).find(|&pen| pens[pen] == colorref) {
            return pen as u16;
        }
        let used = self.used_colors();
        let allocated = self.allocated.clone();
        if let Some(sxf) = self.file.header.settings.as_mut().and_then(|settings| settings.sxf.as_mut()) {
            if let Some(slot) = (1..sxf.pens.len().min(257)).find(|&slot| sxf.pens[slot].color & 0xff_ffff == colorref)
            {
                return 100 + slot as u16;
            }
            // ユーザー定義の色（SXF の 17〜255）のうち、図形が使っていない名前のない色
            if let Some(slot) = (17..sxf.pens.len().min(256)).find(|&slot| {
                let pen = 100 + slot as u16;
                !used.contains(&pen) && !allocated.contains(&pen) && sxf_free(sxf, slot)
            }) {
                sxf.pens[slot].color = colorref;
                if let Some(printer) = sxf.printer_pens.get_mut(slot) {
                    printer.color = colorref;
                }
                let pen = 100 + slot as u16;
                self.allocated.insert(pen);
                return pen;
            }
        }
        self.notices.add(
            NoticeLevel::Info,
            "edit-nearest-color",
            "JWW の線色に同じ色がなく、SXF 対応色も足せないため、最も近い線色にしました",
        );
        nearest_pen(&pens, rgb)
    }

    /// 図形が使う線色
    fn used_colors(&self) -> BTreeSet<u16> {
        let top = self.slots.iter().filter(|slot| !slot.deleted).map(|slot| &slot.record.entity);
        let inside = self.file.blocks.iter().flat_map(|block| block.entities.iter()).map(|record| &record.entity);
        top.chain(inside).flat_map(bases).map(|base| base.pen_color).collect()
    }

    /// 図形ごとの線幅（JWW の線幅の単位。1/100 mm か、印刷のドット）
    fn pen_width(&mut self, index: usize, width: EditLineWidth) -> Result<Option<u16>, EditError> {
        let mm = match width {
            EditLineWidth::Default => return Ok(Some(0)),
            EditLineWidth::ByLayer | EditLineWidth::ByBlock => {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    "JWW の線幅にレイヤ・ブロックの線幅（bylayer・byblock）はありません",
                ));
            }
            EditLineWidth::Millimeters(mm) => mm,
        };
        if self.file.version < 351 {
            self.notices.add(
                NoticeLevel::Info,
                "edit-no-lineweight",
                "版 351 より前の JWW は図形の線幅を持たないため、書きませんでした",
            );
            return Ok(None);
        }
        let general = self.settings().map(|settings| &settings.general);
        let hundredth = general.is_some_and(|general| general.max_draw_width < 0);
        let dpi = self
            .settings()
            .and_then(|settings| settings.display.as_ref())
            .filter(|_| self.file.version >= 600)
            .and_then(|display| match display.print_display_only {
                10..20 => Some(300.0),
                20..30 => Some(600.0),
                _ => None,
            });
        let wanted = if hundredth {
            mm * 100.0
        } else if let Some(dpi) = dpi {
            mm * dpi / 25.4
        } else {
            self.notices.add(
                NoticeLevel::Info,
                "edit-no-lineweight",
                "この JWW は線幅の単位（1/100 mm か印刷の dpi）が分からないため、線幅を書きませんでした",
            );
            return Ok(None);
        };
        let value = wanted.round().clamp(1.0, f64::from(u16::MAX));
        if (value - wanted).abs() > 1e-6 {
            self.notices.add(
                NoticeLevel::Info,
                "edit-rounded-lineweight",
                "線幅を、JWW に書ける最も近い太さにしました",
            );
        }
        Ok(Some(value as u16))
    }

    /// 共通の値を属性に当てる
    fn apply_common(
        &mut self,
        index: usize,
        base: &mut JwwBase,
        common: &Common,
        style: bool,
    ) -> Result<(), EditError> {
        if let Some((group, layer)) = common.layer {
            base.layer_group = group as u16;
            base.layer = layer as u16;
        }
        if let Some(color) = common.color {
            base.pen_color = self.pen_color(index, color)?;
        }
        if style && let Some(line_type) = common.line_type {
            base.pen_style = line_type;
        }
        if let Some(width) = common.line_width
            && let Some(width) = self.pen_width(index, width)?
        {
            base.pen_width = width;
        }
        Ok(())
    }

    // ---- 操作 ----

    fn add(&mut self, index: usize, kind: &str, container: Option<&str>, values: &EditValues) -> Result<(), EditError> {
        if !ADDABLE.contains(&kind) {
            let message = if kind == "polyline" {
                "JWW には折線がありません。線（line）を頂点の数だけ足してください".to_string()
            } else {
                format!("JWW の編集では {kind} を足せません（足せるのは {}）", ADDABLE.join("、"))
            };
            return Err(EditError::at(index, "unsupported-kind", message));
        }
        let reader = ValueReader::new(values, index);
        let (own, style, width): (&[&str], bool, bool) = match kind {
            "line" => (&["start", "end"], true, true),
            "circle" => (&["center", "radius"], true, true),
            "arc" => (&["center", "radius", "angle"], true, true),
            "ellipse" | "ellipse-arc" => (&["center", "majorAxis", "ratio", "parameter"], true, true),
            "point" => (&["position"], false, true),
            "text" => (&["position", "text", "height", "width", "spacing", "rotation", "font"], false, false),
            "dimension" => (&zumen_core::edit::dimension::KEYS, true, true),
            "hatch" => (&zumen_core::edit::hatch::KEYS, true, true),
            _ => (&["block", "position", "rotation", "scale"], false, false),
        };
        let mut keys: Vec<&str> = vec!["layer"];
        if kind != BLOCK_REFERENCE {
            keys.push("color");
        }
        if style {
            keys.push("lineType");
        }
        if width {
            keys.push("lineWidth");
        }
        keys.extend_from_slice(own);
        reader.allow(&keys, &format!("JWW の {kind}"))?;
        let target = self.container(index, container)?;
        let group = match target {
            Target::List(group) => group,
            Target::Block(_) => None,
        };
        let mut common = self.common(index, &reader, group)?;
        let default_group = group.unwrap_or_else(|| self.write_group());
        let layer = *common.layer.get_or_insert_with(|| (default_group, self.write_layer(default_group)));
        if group.is_some_and(|group| group != layer.0) {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!(
                    "レイヤ {:X}-{:X} は container のレイヤグループ {:X} にありません",
                    layer.0, layer.1, default_group
                ),
            ));
        }
        // 用紙の座標: 実寸をレイヤグループの縮尺で割る。ブロック定義の中は、定義の基点を原点にする
        let factor = 1.0 / self.scale(layer.0);
        let origin = match target {
            Target::Block(block) => self.block_bases.get(&self.file.blocks[block].number).copied().unwrap_or([0.0; 2]),
            Target::List(_) => [0.0; 2],
        };
        let paper = move |p: [f64; 2]| [(p[0] - origin[0]) * factor, (p[1] - origin[1]) * factor];
        let point = |key: &str| -> Result<(f64, f64), EditError> {
            let point = reader.required(key, reader.point(key)?)?;
            let [x, y] = paper([point[0], point[1]]);
            Ok((x, y))
        };
        if kind == "dimension" || kind == "hatch" {
            let entities = if kind == "dimension" {
                vec![self.dimension(index, &reader, &common, layer, &paper)?]
            } else {
                self.hatch(index, &reader, &common, layer, &paper, factor)?
            };
            self.push(target, entities);
            return Ok(());
        }
        let mut base = JwwBase { pen_style: 1, pen_color: DEFAULT_PEN_COLOR, ..JwwBase::default() };
        self.apply_common(index, &mut base, &common, style)?;
        let entity = match kind {
            "line" => {
                let (x1, y1) = point("start")?;
                let (x2, y2) = point("end")?;
                JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 })
            }
            "circle" | "arc" => {
                let (cx, cy) = point("center")?;
                let radius = reader.required("radius", reader.positive("radius")?)? * factor;
                let (start, sweep, full) = if kind == "arc" {
                    let angle = reader.required("angle", reader.numbers("angle", 2)?)?;
                    (angle[0].to_radians(), ccw_sweep(angle[0], angle[1]).to_radians(), false)
                } else {
                    (0.0, TAU, true)
                };
                JwwEntity::Arc(JwwArc { base, cx, cy, radius, start, sweep, tilt: 0.0, flatness: 1.0, full })
            }
            "ellipse" | "ellipse-arc" => {
                let (cx, cy) = point("center")?;
                let axis = reader.required("majorAxis", reader.point("majorAxis")?)?;
                let ratio = reader.required("ratio", reader.positive("ratio")?)?;
                let length = axis[0].hypot(axis[1]);
                if ratio > 1.0 || length == 0.0 {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        "ratio は 0 より大きく 1 以下、majorAxis は 0 でない長さにしてください",
                    ));
                }
                let (start, sweep, full) = match reader.numbers("parameter", 2)? {
                    Some(parameter) => (parameter[0], ccw_parameter(parameter[0], parameter[1]), false),
                    None if kind == "ellipse" => (0.0, TAU, true),
                    None => return Err(EditError::at(index, "invalid-value", "parameter がありません")),
                };
                let tilt = axis[1].atan2(axis[0]);
                JwwEntity::Arc(JwwArc {
                    base,
                    cx,
                    cy,
                    radius: length * factor,
                    start,
                    sweep,
                    tilt,
                    flatness: ratio,
                    full,
                })
            }
            "point" => {
                let (x, y) = point("position")?;
                JwwEntity::Point(JwwPoint { base, x, y, temporary: false, code: 0, angle: 0.0, scale: 0.0 })
            }
            "text" => {
                let (x1, y1) = point("position")?;
                let content = reader.required("text", reader.text("text")?)?;
                check_text(index, content)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let size_x = reader.positive("width")?.unwrap_or(height);
                let spacing = non_negative(index, &reader, "spacing")?.unwrap_or(0.0);
                let font = reader.name("font")?.unwrap_or(DEFAULT_FONT);
                let mut text = JwwText {
                    base,
                    x1,
                    y1,
                    x2: x1,
                    y2: y1,
                    text_type: 0,
                    size_x,
                    size_y: height,
                    spacing,
                    angle: reader.number("rotation")?.unwrap_or(0.0),
                    font: font.to_string(),
                    content: content.to_string(),
                };
                text_end(&mut text);
                JwwEntity::Text(text)
            }
            _ => {
                let name = reader.required("block", reader.name("block")?)?;
                let definition = self.block(index, name)?;
                // ブロックの中に、そのブロックを含むブロックは置けない（配置が循環する）
                if let Target::Block(block) = target {
                    let container = self.file.blocks[block].number;
                    if definition == container || self.contains_block(definition, container, 0) {
                        return Err(EditError::at(
                            index,
                            "invalid-value",
                            format!("ブロック {name} はこのブロックを含むため、この中に置けません"),
                        ));
                    }
                }
                let (x, y) = point("position")?;
                let (scale_x, scale_y) = block_scale(index, &reader)?.unwrap_or((1.0, 1.0));
                let rotation = reader.number("rotation")?.unwrap_or(0.0).to_radians();
                self.mark_referenced(definition);
                JwwEntity::Block(JwwBlock { base, x, y, scale_x, scale_y, rotation, definition })
            }
        };
        self.push(target, vec![entity]);
        Ok(())
    }

    /// 足した図形を、足す先の最後に入れる。created は最初の図形の id
    fn push(&mut self, target: Target, entities: Vec<JwwEntity>) {
        match target {
            Target::List(_) => {
                let first = self.slots.len();
                let records = entities.into_iter().map(|entity| Slot {
                    record: JwwRecord::new(entity),
                    origin: None,
                    deleted: false,
                });
                self.slots.extend(records);
                self.created.push(Created::Entity(first));
            }
            Target::Block(block) => {
                let definition = &mut self.file.blocks[block];
                let first = definition.entities.len() + 1;
                definition.entities.extend(entities.into_iter().map(JwwRecord::new));
                let name = plain_name(&definition.name).to_string();
                self.created.push(Created::Member(name, first));
            }
        }
    }

    /// 寸法図形（CDataSunpou）。大きさ・色・寸法値の書き方は図面の寸法の設定（docs/decisions/0025）
    fn dimension(
        &mut self,
        index: usize,
        reader: &ValueReader<'_>,
        common: &Common,
        layer: (usize, usize),
        paper: &dyn Fn([f64; 2]) -> [f64; 2],
    ) -> Result<JwwEntity, EditError> {
        let input = DimensionInput::read(reader, index)?;
        if matches!(input, DimensionInput::Angular { .. }) {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "JWW の寸法図形は直線の寸法線だけを持つため、角度寸法（angular-3-point）は足せません",
            ));
        }
        let text = reader.text("text")?.unwrap_or("<>");
        if reader.name("dimensionStyle")?.is_some() {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-style",
                "JWW は名前の付いた寸法スタイルを持たないため、図面の寸法の設定で描きました",
            );
        }
        let geometry = input.geometry().map_err(|message| EditError::at(index, "invalid-value", message))?;
        let (measurement, geometry) = match geometry {
            DimensionGeometry::Linear { origins, feet, direction, measurement } => (
                measurement,
                DimensionGeometry::Linear {
                    origins: origins.map(paper),
                    feet: feet.map(paper),
                    direction,
                    measurement,
                },
            ),
            DimensionGeometry::Radial { center, from, to, measurement, diameter } => (
                measurement,
                DimensionGeometry::Radial {
                    center: paper(center),
                    from: paper(from),
                    to: paper(to),
                    measurement,
                    diameter,
                },
            ),
            DimensionGeometry::Angular { .. } => unreachable!("角度寸法は上で断った"),
        };
        let settings = dimension::Settings::read(self.settings());
        if settings.end != 0 {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-end",
                "JWW の寸法図形は寸法線の端に点を持ち矢印を持たないため、寸法の設定の矢印・逆矢印は点にしました",
            );
        }
        let color = match common.color {
            Some(color) => Some(self.pen_color(index, color)?),
            None => None,
        };
        let width = match common.line_width {
            Some(width) => self.pen_width(index, width)?,
            None => None,
        };
        let look =
            dimension::Look { layer: (layer.0 as u16, layer.1 as u16), color, line_type: common.line_type, width };
        let entity = dimension::dimension(&settings, &geometry, measurement, text, &look, text_end);
        check_text(index, &entity.text.content)?;
        Ok(JwwEntity::Dimension(entity))
    }

    /// ハッチング。JWW にハッチングの図形はないので、塗りは台形のソリッド、模様は領域で切ったハッチの線と点に
    /// する（docs/decisions/0025）
    fn hatch(
        &mut self,
        index: usize,
        reader: &ValueReader<'_>,
        common: &Common,
        layer: (usize, usize),
        paper: &dyn Fn([f64; 2]) -> [f64; 2],
        factor: f64,
    ) -> Result<Vec<JwwEntity>, EditError> {
        let input = zumen_core::edit::hatch::read(reader, index)?;
        let rings: Vec<Vec<f64>> = input
            .boundaries
            .iter()
            .map(|ring| ring.iter().flat_map(|point| paper([point[0], point[1]])).collect())
            .collect();
        let base = JwwBase {
            pen_style: 1,
            pen_color: DEFAULT_PEN_COLOR,
            layer: layer.1 as u16,
            layer_group: layer.0 as u16,
            ..JwwBase::default()
        };
        if input.solid {
            let quads = hatch::trapezoids(&rings).ok_or_else(|| {
                EditError::at(index, "invalid-value", "boundaries の辺が多すぎて、ソリッドに分けられません")
            })?;
            if quads.is_empty() {
                return Err(EditError::at(index, "invalid-value", "boundaries の境界が囲む面積がありません"));
            }
            // ソリッドの #rrggbb は任意色（線色 10 と RGB）
            let (pen_color, color) = match common.color {
                Some(EditColor::Rgb(rgb)) => (10, Some(((rgb >> 16) & 0xff) | (rgb & 0xff00) | ((rgb & 0xff) << 16))),
                Some(color) => (self.pen_color(index, color)?, None),
                None => (DEFAULT_PEN_COLOR, None),
            };
            if common.line_type.is_some() || common.line_width.is_some() {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-unused-attributes",
                    "JWW のソリッドは線種・線幅を持たないため、塗りのハッチングの線種・線幅の指定を使っていません",
                );
            }
            if quads.len() > 1 {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-fill-solids",
                    "JWW にはハッチングの図形がないため、塗りを横の帯ごとの台形のソリッドに分けて書きました（created は最初のソリッドで、残りはその後に続きます）",
                );
            }
            let base = JwwBase { pen_color, ..base };
            return Ok(quads.into_iter().map(|points| JwwEntity::Solid(JwwSolid { base, points, color })).collect());
        }
        let lines: Vec<PatternLine> = input
            .lines
            .iter()
            .map(|line| PatternLine {
                angle: line.angle,
                base: paper(line.base),
                offset: [line.offset[0] * factor, line.offset[1] * factor],
                dashes: line.dashes.iter().map(|dash| dash * factor).collect(),
            })
            .collect();
        let pieces = hatch::pattern_pieces(&lines, &rings).ok_or_else(|| {
            EditError::at(
                index,
                "invalid-value",
                format!(
                    "模様の線が {} 本を超えるか、間隔が 0 の線があります。patternScale を大きくしてください",
                    hatch::MAX_PATTERN_ENTITIES
                ),
            )
        })?;
        if pieces.is_empty() {
            return Err(EditError::at(
                index,
                "invalid-value",
                "境界の内側に模様の線がありません。patternScale を小さくしてください",
            ));
        }
        let mut base = JwwBase { flag: HATCH_FLAG, ..base };
        self.apply_common(index, &mut base, common, true)?;
        self.notices.add(
            NoticeLevel::Info,
            "edit-hatch-lines",
            "JWW にはハッチングの図形がないため、模様を境界で切ったハッチの線と点にしました（created は最初の線で、残りはその後に続きます）",
        );
        Ok(pieces
            .into_iter()
            .map(|piece| match piece {
                hatch::Piece::Line([x1, y1, x2, y2]) => JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 }),
                hatch::Piece::Point([x, y]) => JwwEntity::Point(JwwPoint {
                    base: JwwBase { pen_style: 1, ..base },
                    x,
                    y,
                    temporary: false,
                    code: 0,
                    angle: 0.0,
                    scale: 0.0,
                }),
            })
            .collect())
    }

    /// 空のブロック定義を足す（docs/decisions/0017）。JWW の定義は基点を持たない（定義の座標の原点が基点）ので、
    /// basePoint は後の操作で中に足す図形の座標から引く。名前は Jw_cad と同じく、版 4.10 から作図部品の印
    /// （"@@SfigorgFlag@@4"）を後ろに付けて書く
    fn add_block(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        let reader = ValueReader::new(values, index);
        reader.allow(&["basePoint"], "JWW のブロック定義")?;
        let name = name.trim();
        if name.is_empty() || name.chars().any(char::is_control) || name.contains(SFIGORG) {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("ブロックの名前 {name} は使えません（空、制御文字、{SFIGORG} を含む名前は使えません）"),
            ));
        }
        if !self.find_blocks(name).is_empty() {
            return Err(EditError::at(index, "already-exists", format!("ブロック {name} はすでにあります")));
        }
        let base = reader.point("basePoint")?.unwrap_or([0.0; 3]);
        // 定義の通し番号は、ほかの定義と重ならない番号（Jw_cad は 0 から振る）
        let number = match self.file.blocks.iter().map(|block| block.number).max() {
            None => 0,
            Some(max) => max.checked_add(1).ok_or_else(|| {
                EditError::at(index, "name-exhausted", "新しいブロック定義の通し番号を割り当てられません")
            })?,
        };
        let stored = if self.file.version >= 410 { format!("{name}{SFIGORG}4") } else { name.to_string() };
        self.file.blocks.push(JwwBlockRecord::new(number, stored, Vec::new()));
        self.block_bases.insert(number, [base[0], base[1]]);
        self.created.push(Created::Block(name.to_string()));
        Ok(())
    }

    /// 名前（後ろの "@@SfigorgFlag@@" と種別は省いてもよい）が合うブロック定義の添字
    fn find_blocks(&self, name: &str) -> Vec<usize> {
        let name = name.trim();
        let exact: Vec<usize> =
            (0..self.file.blocks.len()).filter(|&i| self.file.blocks[i].name.trim() == name).collect();
        if !exact.is_empty() {
            return exact;
        }
        (0..self.file.blocks.len()).filter(|&i| plain_name(&self.file.blocks[i].name) == plain_name(name)).collect()
    }

    /// 定義 definition の中（入れ子も）に、定義 target の配置があるか
    fn contains_block(&self, definition: u32, target: u32, depth: usize) -> bool {
        if depth > MAX_BLOCK_DEPTH {
            return true;
        }
        self.file.blocks.iter().filter(|block| block.number == definition).any(|block| {
            block.entities.iter().any(|record| match &record.entity {
                JwwEntity::Block(inner) => {
                    inner.definition == target || self.contains_block(inner.definition, target, depth + 1)
                }
                _ => false,
            })
        })
    }

    /// ブロック定義の通し番号。名前（後ろの "@@SfigorgFlag@@" と種別は省いてもよい）で選ぶ
    fn block(&self, index: usize, name: &str) -> Result<u32, EditError> {
        match self.find_blocks(name)[..] {
            [block] => Ok(self.file.blocks[block].number),
            [] => {
                Err(EditError::at(index, "missing-definition", format!("ブロック {} は図面にありません", name.trim())))
            }
            _ => Err(EditError::at(index, "invalid-value", format!("ブロック {} が 2 つ以上あります", name.trim()))),
        }
    }

    /// ブロック定義を「参照されている」にする
    fn mark_referenced(&mut self, number: u32) {
        for block in self.file.blocks.iter_mut().filter(|block| block.number == number) {
            block.referenced = true;
        }
    }

    /// id を図形の一覧の場所にする。id は読んだときの図面の id（#5）。ブロックの中（#5>#1）は選べない
    fn target(&self, index: usize, id: &str) -> Result<usize, EditError> {
        let id = id.trim();
        if let Some((placement, _)) = id.split_once('>') {
            return Err(EditError::at(
                index,
                "inside-block",
                format!(
                    "図形 {id} はブロック {placement} の中にあります。JWW の編集ではブロックの配置（id {placement}）だけを変えられます（定義を変えると、そのブロックのすべての配置が変わります）"
                ),
            ));
        }
        let number = id.strip_prefix('#').and_then(|number| number.parse::<usize>().ok());
        number
            .and_then(|number| self.slots.iter().position(|slot| slot.origin == Some(number) && !slot.deleted))
            .ok_or_else(|| EditError::at(index, "not-found", format!("図形 {id} は図面にありません")))
    }

    fn update(&mut self, index: usize, id: &str, set: &EditValues) -> Result<(), EditError> {
        let slot = self.target(index, id)?;
        let kind = kind_of(&self.slots[slot].record.entity);
        let reader = ValueReader::new(set, index);
        let (own, color, style, width): (&[&str], bool, bool, bool) = match kind {
            "line" => (&["start", "end"], true, true, true),
            "circle" => (&["center", "radius"], true, true, true),
            "arc" => (&["center", "radius", "angle"], true, true, true),
            "ellipse" => (&["center", "majorAxis", "ratio"], true, true, true),
            "ellipse-arc" => (&["center", "majorAxis", "ratio", "parameter"], true, true, true),
            "point" => (&["position"], true, false, true),
            "text" => (&["position", "text", "height", "width", "spacing", "rotation", "font"], true, false, false),
            "solid" | "dimension" => (&[], true, false, false),
            _ => (&["block", "position", "rotation", "scale"], false, false, false),
        };
        let mut keys: Vec<&str> = vec!["layer"];
        if color {
            keys.push("color");
        }
        if style {
            keys.push("lineType");
        }
        if width {
            keys.push("lineWidth");
        }
        keys.extend_from_slice(own);
        reader.allow(&keys, &format!("JWW の {kind} の変更"))?;
        let common = self.common(index, &reader, None)?;
        let mut entity = self.slots[slot].record.entity.clone();
        let old_group = usize::from(entity.base().layer_group & 0x0f);
        // 縮尺の違うレイヤグループに移すときは、実寸を保つ
        if let Some((group, _)) = common.layer
            && self.scale(group) != self.scale(old_group)
        {
            scale_entity(&mut entity, self.scale(old_group) / self.scale(group));
            self.notices.add(
                NoticeLevel::Info,
                "edit-rescaled",
                "縮尺の違うレイヤグループに移した図形は、実寸が変わらないように用紙の座標を変えました",
            );
        }
        // ソリッドの #rrggbb は任意色（線色 10 と RGB）
        let solid_color = match (&mut entity, common.color) {
            (JwwEntity::Solid(e), Some(EditColor::Rgb(rgb))) => Some((&mut e.base, &mut e.color, rgb)),
            (JwwEntity::CircleSolid(e), Some(EditColor::Rgb(rgb))) => Some((&mut e.base, &mut e.color, rgb)),
            _ => None,
        };
        let common = match solid_color {
            Some((base, color, rgb)) => {
                base.pen_color = 10;
                *color = Some(((rgb >> 16) & 0xff) | (rgb & 0xff00) | ((rgb & 0xff) << 16));
                Common { color: None, ..common }
            }
            None => common,
        };
        for base in bases_mut(&mut entity) {
            self.apply_common(index, base, &common, style)?;
        }
        let group = usize::from(entity.base().layer_group & 0x0f);
        let factor = 1.0 / self.scale(group);
        let point = |key: &str| -> Result<Option<(f64, f64)>, EditError> {
            Ok(reader.point(key)?.map(|point| (point[0] * factor, point[1] * factor)))
        };
        match &mut entity {
            JwwEntity::Line(e) => {
                if let Some((x, y)) = point("start")? {
                    (e.x1, e.y1) = (x, y);
                }
                if let Some((x, y)) = point("end")? {
                    (e.x2, e.y2) = (x, y);
                }
            }
            JwwEntity::Arc(e) => {
                if let Some((x, y)) = point("center")? {
                    (e.cx, e.cy) = (x, y);
                }
                if let Some(radius) = reader.positive("radius")? {
                    e.radius = radius * factor;
                }
                if let Some(angle) = reader.numbers("angle", 2)? {
                    // 傾きの分を除いた始角と、反時計回りの円弧角
                    e.start = angle[0].to_radians() - e.tilt;
                    e.sweep = ccw_sweep(angle[0], angle[1]).to_radians();
                }
                if let Some(axis) = reader.point("majorAxis")? {
                    let length = axis[0].hypot(axis[1]);
                    if length == 0.0 {
                        return Err(EditError::at(index, "invalid-value", "majorAxis は 0 でない長さにしてください"));
                    }
                    e.radius = length * factor;
                    e.tilt = axis[1].atan2(axis[0]);
                }
                if let Some(ratio) = reader.positive("ratio")? {
                    if ratio > 1.0 {
                        return Err(EditError::at(
                            index,
                            "invalid-value",
                            "ratio は 0 より大きく 1 以下にしてください",
                        ));
                    }
                    e.flatness = ratio;
                }
                if let Some(parameter) = reader.numbers("parameter", 2)? {
                    e.start = parameter[0];
                    e.sweep = ccw_parameter(parameter[0], parameter[1]);
                }
            }
            JwwEntity::Point(e) => {
                if let Some((x, y)) = point("position")? {
                    (e.x, e.y) = (x, y);
                }
            }
            JwwEntity::Text(e) => {
                if let Some((x, y)) = point("position")? {
                    let (dx, dy) = (e.x2 - e.x1, e.y2 - e.y1);
                    (e.x1, e.y1) = (x, y);
                    (e.x2, e.y2) = (x + dx, y + dy);
                }
                let mut resized = false;
                if let Some(text) = reader.text("text")? {
                    check_text(index, text)?;
                    e.content = text.to_string();
                    resized = true;
                }
                if let Some(height) = reader.positive("height")? {
                    // 幅の指定がなければ、幅と高さの比を保つ
                    if reader.value("width").is_none() && e.size_y > 0.0 {
                        e.size_x *= height / e.size_y;
                    }
                    e.size_y = height;
                    resized = true;
                }
                if let Some(width) = reader.positive("width")? {
                    e.size_x = width;
                    resized = true;
                }
                if let Some(spacing) = non_negative(index, &reader, "spacing")? {
                    e.spacing = spacing;
                    resized = true;
                }
                if let Some(rotation) = reader.number("rotation")? {
                    e.angle = rotation;
                    resized = true;
                }
                if let Some(font) = reader.name("font")? {
                    e.font = font.to_string();
                }
                if resized {
                    text_end(e);
                }
            }
            JwwEntity::Block(e) => {
                if let Some(name) = reader.name("block")? {
                    e.definition = self.block(index, name)?;
                }
                if let Some((x, y)) = point("position")? {
                    (e.x, e.y) = (x, y);
                }
                if let Some((x, y)) = block_scale(index, &reader)? {
                    (e.scale_x, e.scale_y) = (x, y);
                }
                if let Some(rotation) = reader.number("rotation")? {
                    e.rotation = rotation.to_radians();
                }
            }
            JwwEntity::Solid(_) | JwwEntity::CircleSolid(_) | JwwEntity::Dimension(_) => {}
        }
        if let JwwEntity::Block(block) = &entity {
            self.mark_referenced(block.definition);
        }
        self.slots[slot].record.entity = entity;
        Ok(())
    }

    fn delete(&mut self, index: usize, id: &str) -> Result<(), EditError> {
        let slot = self.target(index, id)?;
        self.slots[slot].deleted = true;
        Ok(())
    }

    /// レイヤを足す。JWW のレイヤは 16 のレイヤグループの 16 のレイヤで、名前も図形もないレイヤに名前を付ける。
    /// group で選ぶレイヤグループ（省略は書込みレイヤグループ）の中から、番号の小さい順に探す
    fn add_layer(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(usize, usize), EditError> {
        if !valid_layer_name(name) {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("レイヤの名前 {name} は使えません（空・制御文字・「0-A」のような番号の形は使えません）"),
            ));
        }
        if self.find_layer(index, &EditValue::Text(name.to_string()))?.is_some() {
            return Err(EditError::at(index, "already-exists", format!("レイヤ {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&LAYER_KEYS, "JWW のレイヤ")?;
        let group = match reader.value("group") {
            Some(value) => self.group(index, value)?,
            None => self.write_group(),
        };
        let used = self.used_layers();
        // 書込みレイヤは非表示・表示のみにできないので、そうするレイヤには使わない
        let hidden = reader.bool("visible")? == Some(false) || reader.bool("locked")? == Some(true);
        let write_layer = self.write_layer(group);
        let layer = (0..16)
            .filter(|&layer| !(hidden && layer == write_layer))
            .find(|&layer| self.layer_name(group, layer).trim().is_empty() && !used.contains(&(group, layer)))
            .ok_or_else(|| {
                EditError::at(
                    index,
                    "invalid-value",
                    format!(
                        "レイヤグループ {}（{group:X}）に、名前も図形もないレイヤがありません",
                        self.group_name(group)
                    ),
                )
            })?;
        self.set_layer_name(index, group, layer, name)?;
        self.apply_layer_values(index, group, layer, &reader)?;
        Ok((group, layer))
    }

    fn update_layer(&mut self, index: usize, name: &str, set: &EditValues) -> Result<(), EditError> {
        let (group, layer) = self
            .find_layer(index, &EditValue::Text(name.to_string()))?
            .ok_or_else(|| EditError::at(index, "not-found", format!("レイヤ {name} は図面にありません")))?;
        let reader = ValueReader::new(set, index);
        let mut keys = LAYER_KEYS.to_vec();
        keys.retain(|key| *key != "group");
        keys.push("name");
        reader.allow(&keys, "JWW のレイヤの変更")?;
        if let Some(new_name) = reader.text("name")? {
            if !new_name.trim().is_empty() {
                if !valid_layer_name(new_name) {
                    return Err(EditError::at(index, "invalid-value", format!("レイヤの名前 {new_name} は使えません")));
                }
                if self
                    .find_layer(index, &EditValue::Text(new_name.to_string()))?
                    .is_some_and(|found| found != (group, layer))
                {
                    return Err(EditError::at(index, "already-exists", format!("レイヤ {new_name} はすでにあります")));
                }
            }
            self.set_layer_name(index, group, layer, new_name)?;
        }
        self.apply_layer_values(index, group, layer, &reader)
    }

    /// レイヤの表示（visible は非表示、locked は表示のみ）とレイヤグループの縮尺。JWW のレイヤにない値は知らせる
    fn apply_layer_values(
        &mut self,
        index: usize,
        group: usize,
        layer: usize,
        reader: &ValueReader<'_>,
    ) -> Result<(), EditError> {
        if LAYER_IGNORED.iter().any(|key| reader.value(key).is_some()) {
            self.notices.add(
                NoticeLevel::Info,
                "edit-layer-attributes",
                "JWW のレイヤは色・線種・線幅・凍結・印刷の設定を持たないため、書きませんでした",
            );
        }
        let visible = reader.bool("visible")?;
        let locked = reader.bool("locked")?;
        if let Some(scale) = reader.positive("scale")? {
            self.set_group_scale(index, group, scale)?;
        }
        let write_layer = self.write_layer(group);
        let Some(state) = self.file.header.groups.get_mut(group).and_then(|group| group.layers.get_mut(layer)) else {
            return Ok(());
        };
        let mut value = state.state;
        if let Some(visible) = visible {
            value = match (visible, value) {
                (false, _) => 0,
                (true, 0) => 2,
                (true, other) => other,
            };
        }
        if let Some(locked) = locked {
            value = match (locked, value) {
                (true, 2 | 3) => 1,
                (false, 1) => 2,
                (_, other) => other,
            };
        }
        if layer == write_layer && value < 2 {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!(
                    "レイヤ {group:X}-{layer:X} はレイヤグループの書込みレイヤなので、非表示・表示のみにできません"
                ),
            ));
        }
        // 書込みレイヤは状態 3 のまま
        state.state = if layer == write_layer { 3 } else { value };
        Ok(())
    }

    /// レイヤグループの縮尺。図形のあるレイヤグループは、図形の実寸が変わるので変えられない
    fn set_group_scale(&mut self, index: usize, group: usize, scale: f64) -> Result<(), EditError> {
        if self.scale(group) == scale {
            return Ok(());
        }
        if self.used_layers().iter().any(|(used, _)| *used == group) {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("図形のあるレイヤグループ {}（{group:X}）の縮尺は変えられません", self.group_name(group)),
            ));
        }
        if let Some(state) = self.file.header.groups.get_mut(group) {
            state.scale = scale;
        }
        Ok(())
    }
}

/// 名前がなく、既定の色のままの SXF のユーザー定義の色
fn sxf_free(sxf: &JwwSxfSettings, slot: usize) -> bool {
    sxf.printer_pens.get(slot).is_none_or(|printer| printer.name.trim().is_empty())
}

/// 画面色が最も近い線色 1〜9（色が読めなければ線色 2）
fn nearest_pen(pens: &[u32], rgb: u32) -> u16 {
    let channels = |value: u32| [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
    let target = channels(rgb);
    let mut best = (DEFAULT_PEN_COLOR, u32::MAX);
    for (pen, colorref) in pens.iter().enumerate().take(10).skip(1) {
        let hex = colorref_rgb(*colorref);
        let color = channels(hex);
        let distance: u32 = (0..3).map(|i| color[i].abs_diff(target[i]).pow(2)).sum();
        if distance < best.1 {
            best = (pen as u16, distance);
        }
    }
    best.0
}

/// COLORREF を 0xRRGGBB にする
fn colorref_rgb(value: u32) -> u32 {
    u32::from_str_radix(colorref(value).trim_start_matches('#'), 16).unwrap_or(0)
}

/// ブロック定義の名前から、後ろの複合図形の種別（"@@SfigorgFlag@@" と番号）を除いたもの
fn plain_name(name: &str) -> &str {
    name.split(SFIGORG).next().unwrap_or(name).trim()
}

/// 1 行の文字か（JWW の文字は 1 行。^@ で始まる文字は Jw_cad の画像などの指定）
fn check_text(index: usize, text: &str) -> Result<(), EditError> {
    if text.is_empty() {
        return Err(EditError::at(index, "invalid-value", "空の文字は書けません（Jw_cad は空の文字を消します）"));
    }
    if text.chars().any(char::is_control) {
        return Err(EditError::at(index, "invalid-value", "JWW の文字は 1 行で、改行などの制御文字は書けません"));
    }
    if text.starts_with("^@") {
        return Err(EditError::at(index, "invalid-value", "^@ で始まる文字は Jw_cad の画像などの指定なので書けません"));
    }
    Ok(())
}

/// 0 以上の数
fn non_negative(index: usize, reader: &ValueReader<'_>, key: &str) -> Result<Option<f64>, EditError> {
    match reader.number(key)? {
        Some(value) if value < 0.0 => {
            Err(EditError::at(index, "invalid-value", format!("{key} は 0 以上の数にしてください")))
        }
        value => Ok(value),
    }
}

/// ブロックの配置の倍率。数（縦横同じ）か [x, y]
fn block_scale(index: usize, reader: &ValueReader<'_>) -> Result<Option<(f64, f64)>, EditError> {
    let scale = match reader.value("scale") {
        None => return Ok(None),
        Some(EditValue::Number(value)) => (*value, *value),
        Some(_) => {
            let point = reader.required("scale", reader.point("scale")?)?;
            (point[0], point[1])
        }
    };
    if scale.0 == 0.0 || scale.1 == 0.0 {
        return Err(EditError::at(index, "invalid-value", "scale は 0 でない数にしてください"));
    }
    Ok(Some(scale))
}

#[cfg(test)]
mod tests;
