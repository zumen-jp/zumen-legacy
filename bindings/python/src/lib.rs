//! zumen の Python の結合部。図面を読み（`read`）、表示用モデル（`Scene`）を JSON・PNG・SVG・PDF にし、文字と図形を取り出す。
//! 図面を DXF に書き直す（`convert`）。図面をその場で編集し（`edit`）、新しい図面を作る（`create`）。
//! 図面の仕様から、用紙・ビュー・寸法・表のそろった図面を描く（`draft`）。
//! 読み取りと描画の間は GIL を外す（FastAPI のスレッドなどで並行に動く）。

use pyo3::exceptions::{PyImportError, PyOSError, PyUserWarning, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use zumen::checks::{CheckReport, RuleSet, check_scene};
use zumen::compare::{CompareOptions, SceneDiff, compare_scenes, diff_scene};
use zumen::extract::{
    EntityFilter, EntityText, Shape, entity_shapes, entity_text, layer_names, real_shapes, scene_texts, select_entities,
};
use zumen::regions::{ClosedRegion, RegionOptions, RegionShape, RegionText, closed_regions};
use zumen::tables::{RuledTable, TableOptions, ruled_tables};
use zumen::title::{TitleKey, title_fields};
use zumen::{BBox, Background, Property, PropertyRaw, ReadDrawingOptions, SceneEntity};
use zumen_render::{ImageSize, RenderOptions, View};

pyo3::create_exception!(zumen, ReadError, PyValueError, "図面を読めなかった（形式が違う、壊れているなど）");
pyo3::create_exception!(zumen, RenderError, PyValueError, "図面を描けなかった（範囲が空、画像が大きすぎるなど）");
pyo3::create_exception!(
    zumen,
    LimitError,
    ReadError,
    "読み取りの上限（max_bytes・max_entities・timeout）を超えたので途中でやめた。limit に上限の種類（bytes・entities・time）がある"
);

pyo3::create_exception!(
    zumen,
    EditError,
    PyValueError,
    "図面を編集できなかった。code に理由の種類（not-found・unknown-key など）、operation に理由になった操作の番号（0 から。図面全体の理由なら None）がある"
);

pyo3::create_exception!(
    zumen,
    DraftError,
    PyValueError,
    "図面の仕様から描けなかった。code に理由の種類（unknown-type・unknown-key・invalid-value・missing-definition など）、path に仕様の中の場所（sheets[0].views[1].items[4]。全体なら空）がある"
);

/// 編集の失敗を EditError にする
fn edit_error(py: Python<'_>, error: zumen::edit::EditError) -> PyErr {
    let err = EditError::new_err(error.to_string());
    let value = err.value(py);
    if let Err(failure) = value.setattr("code", error.code).and_then(|()| value.setattr("operation", error.operation)) {
        return failure;
    }
    err
}

/// 操作を読む。Python の list と dict（JSON にできる値）か、JSON の文字列
fn edit_operations(py: Python<'_>, operations: &Bound<'_, PyAny>) -> PyResult<Vec<zumen::edit::EditOperation>> {
    let text: String = match operations.extract::<String>() {
        Ok(text) => text,
        Err(_) => py.import("json")?.call_method1("dumps", (operations,))?.extract()?,
    };
    zumen::edit::parse_operations(&text).map_err(|error| edit_error(py, error))
}

/// 読み取りの失敗を Python の例外にする。上限を超えたものは LimitError にして、limit に種類を入れる
fn read_error(py: Python<'_>, error: zumen::ReadError) -> PyErr {
    match error.exceeded() {
        Some(limit) => {
            let err = LimitError::new_err(error.message().to_string());
            if let Err(failure) = err.value(py).setattr("limit", limit.name()) {
                return failure;
            }
            err
        }
        None => ReadError::new_err(error.message().to_string()),
    }
}

/// 形式に依存しない表示用の図面。座標は用紙の mm で、Y 軸は上向き
#[pyclass(frozen, module = "zumen")]
struct Scene {
    inner: Arc<zumen::Scene>,
    /// 図形の id から位置への対応（entity で初めて使うときに作る）
    ids: OnceLock<HashMap<String, usize>>,
}

impl Scene {
    fn new(scene: zumen::Scene) -> Self {
        Self { inner: Arc::new(scene), ids: OnceLock::new() }
    }
}

type Region = (f64, f64, f64, f64);

/// Python の範囲 (min_x, min_y, max_x, max_y) を矩形にする。逆に並べても同じ範囲
fn to_bbox((x0, y0, x1, y1): Region) -> BBox {
    BBox { min_x: x0.min(x1), min_y: y0.min(y1), max_x: x0.max(x1), max_y: y0.max(y1) }
}

fn from_bbox(b: &BBox) -> Region {
    (b.min_x, b.min_y, b.max_x, b.max_y)
}

fn filter(region: Option<Region>, hidden_layers: Option<Vec<u32>>) -> EntityFilter {
    EntityFilter {
        region: region.map(to_bbox),
        hidden_layers: hidden_layers.map(|ids| ids.into_iter().collect::<HashSet<u32>>()),
    }
}

fn parse_theme(theme: Option<&str>) -> PyResult<Option<Background>> {
    match theme {
        None => Ok(None),
        Some("light") => Ok(Some(Background::Light)),
        Some("dark") => Ok(Some(Background::Dark)),
        Some(other) => Err(PyValueError::new_err(format!("theme は light か dark です: {other}"))),
    }
}

/// 同梱のフォントに字形がなかった文字と、描かなかった内容（underlay など）を、Python の警告にする
fn warn_report(py: Python<'_>, report: &zumen_render::RenderReport) -> PyResult<()> {
    let warn = |message: String| -> PyResult<()> {
        PyErr::warn(py, &py.get_type::<PyUserWarning>(), &std::ffi::CString::new(message)?, 1)
    };
    if !report.missing_glyphs.is_empty() {
        let characters: String = report.missing_glyphs.iter().collect();
        warn(format!("同梱のフォントに字形がない文字を描いていません: {characters}"))?;
    }
    for notice in &report.notices {
        warn(format!("{}（{}、{} 件）", notice.message, notice.code, notice.count))?;
    }
    Ok(())
}

fn pairs(points: &[f64]) -> Vec<(f64, f64)> {
    points.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y)).collect()
}

/// 形の座標を key に入れる。polyline は点の並び、segments は 2 点の組の並び、fill と hatch は輪（点の並び）の並び、
/// point と text は 1 点
fn set_coordinates(dict: &Bound<'_, PyDict>, key: &str, shape: &Shape) -> PyResult<()> {
    match shape {
        Shape::Polyline { points, .. } => dict.set_item(key, pairs(points)),
        Shape::Segments { points } => {
            let segments: Vec<((f64, f64), (f64, f64))> =
                points.as_chunks::<4>().0.iter().map(|[x0, y0, x1, y1]| ((*x0, *y0), (*x1, *y1))).collect();
            dict.set_item(key, segments)
        }
        Shape::Fill { rings } | Shape::Hatch { rings } => {
            dict.set_item(key, rings.iter().map(|ring| pairs(ring)).collect::<Vec<_>>())
        }
        Shape::Point { x, y } | Shape::Text { x, y, .. } => dict.set_item(key, (*x, *y)),
    }
}

/// 形の dict。shape（polyline、segments、fill、hatch、point、text）と、用紙の mm の座標（polyline は points、
/// segments は segments、fill と hatch は rings、point と text は point）、polyline の closed、text の text・height・angle。
/// real は同じ座標を配置の元の座標（実寸の mm）にしたもので、配置の中にない図形では None
fn shape_dict<'py>(py: Python<'py>, shape: &Shape, real: Option<&Shape>) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("shape", shape.name())?;
    if let Shape::Text { text, .. } = shape {
        dict.set_item("text", text)?;
    }
    let key = match shape {
        Shape::Polyline { .. } => "points",
        Shape::Segments { .. } => "segments",
        Shape::Fill { .. } | Shape::Hatch { .. } => "rings",
        Shape::Point { .. } | Shape::Text { .. } => "point",
    };
    set_coordinates(&dict, key, shape)?;
    match shape {
        Shape::Polyline { closed, .. } => dict.set_item("closed", *closed)?,
        Shape::Text { height, angle, .. } => {
            dict.set_item("height", *height)?;
            dict.set_item("angle", *angle)?;
        }
        _ => {}
    }
    match real {
        Some(real) => set_coordinates(&dict, "real", real)?,
        None => dict.set_item("real", py.None())?,
    }
    Ok(dict)
}

/// 図形の形の dict の並び（`shape_dict`）
fn geometry_list<'py>(py: Python<'py>, entity: &SceneEntity) -> PyResult<Bound<'py, PyList>> {
    let shapes = entity_shapes(entity, None);
    let real = real_shapes(entity);
    let list = PyList::empty(py);
    for (index, shape) in shapes.iter().enumerate() {
        list.append(shape_dict(py, shape, real.as_ref().and_then(|real| real.get(index)))?)?;
    }
    Ok(list)
}

/// 属性の値。整形する前の値（数・座標など）があればそれ、なければ文字列の値
fn property_value<'py>(py: Python<'py>, property: &Property) -> PyResult<Bound<'py, PyAny>> {
    Ok(match &property.raw {
        None => property.value.as_str().into_pyobject(py)?.into_any(),
        Some(PropertyRaw::Number(value)) => value.into_pyobject(py)?.into_any(),
        Some(PropertyRaw::Text(value)) => value.as_str().into_pyobject(py)?.into_any(),
        Some(PropertyRaw::Bool(value)) => value.into_pyobject(py)?.to_owned().into_any(),
        Some(PropertyRaw::Numbers(values)) => pyo3::types::PyTuple::new(py, values)?.into_any(),
    })
}

/// 属性の key → 値の dict（property_value）
fn property_values<'py>(py: Python<'py>, properties: &[Property]) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    for property in properties {
        dict.set_item(property.key, property_value(py, property)?)?;
    }
    Ok(dict)
}

/// 配置の dict（name、scale_x、scale_y、inverse）
fn placement_dict<'py>(py: Python<'py>, placement: &zumen::Placement) -> PyResult<Bound<'py, PyDict>> {
    let item = PyDict::new(py);
    item.set_item("name", &placement.name)?;
    item.set_item("scale_x", placement.scale_x)?;
    item.set_item("scale_y", placement.scale_y)?;
    let [a, b, c, d, e, f] = placement.inverse;
    item.set_item("inverse", (a, b, c, d, e, f))?;
    Ok(item)
}

/// 図形の dict（id、kind、type、source、layers、bbox、text、properties、values、placement、geometry を求めたときは geometry）
fn entity_dict<'py>(
    py: Python<'py>,
    scene: &zumen::Scene,
    entity: &SceneEntity,
    geometry: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("kind", entity.kind.name())?;
    dict.set_item("type", &entity.type_name)?;
    dict.set_item("source", &entity.source)?;
    dict.set_item("layers", layer_names(scene, entity))?;
    dict.set_item("bbox", from_bbox(&entity.bbox))?;
    dict.set_item("text", entity_text(entity).map(|text| text.text))?;
    let properties: Vec<(&str, &str)> =
        entity.properties.iter().map(|property| (property.label.as_str(), property.value.as_str())).collect();
    dict.set_item("properties", properties)?;
    dict.set_item("values", property_values(py, &entity.properties)?)?;
    dict.set_item(
        "placement",
        entity.placement.as_deref().map(|placement| placement_dict(py, placement)).transpose()?,
    )?;
    if geometry {
        dict.set_item("geometry", geometry_list(py, entity)?)?;
    }
    Ok(dict)
}

/// 領域の形と大きさの dict（area、outline_area、perimeter、outline、holes）
fn region_shape_dict<'py>(py: Python<'py>, shape: &RegionShape) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("area", shape.area)?;
    dict.set_item("outline_area", shape.outline_area)?;
    dict.set_item("perimeter", shape.perimeter)?;
    dict.set_item("outline", pairs(&shape.outline))?;
    dict.set_item("holes", shape.holes.iter().map(|hole| pairs(hole)).collect::<Vec<_>>())?;
    Ok(dict)
}

/// 閉じた領域の dict（用紙の形と大きさに bbox、placement、real、boundary、texts を加える）
fn region_dict<'py>(py: Python<'py>, region: &ClosedRegion) -> PyResult<Bound<'py, PyDict>> {
    let dict = region_shape_dict(py, &region.paper)?;
    dict.set_item("bbox", from_bbox(&region.bbox))?;
    dict.set_item(
        "placement",
        region.placement.as_deref().map(|placement| placement_dict(py, placement)).transpose()?,
    )?;
    dict.set_item("real", region.real.as_ref().map(|real| region_shape_dict(py, real)).transpose()?)?;
    dict.set_item("boundary", &region.boundary)?;
    dict.set_item("texts", region_texts(py, &region.texts)?)?;
    Ok(dict)
}

/// 領域やます目の中の文字の list（id、text、文字範囲の中心 x・y）
fn region_texts<'py>(py: Python<'py>, texts: &[RegionText]) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for text in texts {
        let item = PyDict::new(py);
        item.set_item("id", &text.entity)?;
        item.set_item("text", &text.text)?;
        item.set_item("x", text.x)?;
        item.set_item("y", text.y)?;
        list.append(item)?;
    }
    Ok(list)
}

/// 規則で調べた結果の dict（rule_set、title、passed、results）
fn check_dict<'py>(py: Python<'py>, report: &CheckReport) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("rule_set", &report.rule_set)?;
    dict.set_item("title", &report.title)?;
    dict.set_item("passed", report.passed())?;
    let results = PyList::empty(py);
    for result in &report.results {
        let item = PyDict::new(py);
        item.set_item("id", &result.id)?;
        item.set_item("kind", result.kind.name())?;
        item.set_item("level", result.level.name())?;
        item.set_item("reference", &result.reference)?;
        item.set_item("description", &result.description)?;
        item.set_item("checked", result.checked)?;
        item.set_item("unchecked", result.unchecked)?;
        let findings = PyList::empty(py);
        for finding in &result.findings {
            let entry = PyDict::new(py);
            entry.set_item("value", &finding.value)?;
            entry.set_item("message", &finding.message)?;
            entry.set_item("count", finding.count)?;
            entry.set_item("entities", &finding.entities)?;
            entry.set_item("layers", &finding.layers)?;
            findings.append(entry)?;
        }
        item.set_item("findings", findings)?;
        results.append(item)?;
    }
    dict.set_item("results", results)?;
    Ok(dict)
}

/// 罫線で描いた表の dict（bbox、placement、row_count、column_count、columns、rows、cells、boundary）
fn table_dict<'py>(py: Python<'py>, table: &RuledTable) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("bbox", from_bbox(&table.bbox))?;
    dict.set_item("placement", table.placement.as_deref().map(|placement| placement_dict(py, placement)).transpose()?)?;
    let (rows, columns) = table.size();
    dict.set_item("row_count", rows)?;
    dict.set_item("column_count", columns)?;
    dict.set_item("columns", &table.columns)?;
    dict.set_item("rows", &table.rows)?;
    let cells = PyList::empty(py);
    for cell in &table.cells {
        let item = PyDict::new(py);
        item.set_item("row", cell.row)?;
        item.set_item("column", cell.column)?;
        item.set_item("row_span", cell.row_span)?;
        item.set_item("column_span", cell.column_span)?;
        item.set_item("bbox", from_bbox(&cell.bbox))?;
        item.set_item("text", &cell.text)?;
        item.set_item("texts", region_texts(py, &cell.texts)?)?;
        cells.append(item)?;
    }
    dict.set_item("cells", cells)?;
    dict.set_item("grid", table.grid())?;
    dict.set_item("boundary", &table.boundary)?;
    Ok(dict)
}

/// 文字の dict（id、kind、type、text、x、y、height、angle、bbox、layers）
fn text_dict<'py>(
    py: Python<'py>,
    scene: &zumen::Scene,
    entity: &SceneEntity,
    text: &EntityText,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("kind", entity.kind.name())?;
    dict.set_item("type", &entity.type_name)?;
    dict.set_item("text", &text.text)?;
    dict.set_item("x", text.x)?;
    dict.set_item("y", text.y)?;
    dict.set_item("height", text.height)?;
    dict.set_item("angle", text.angle)?;
    dict.set_item("bbox", from_bbox(&text.bbox))?;
    dict.set_item("layers", layer_names(scene, entity))?;
    Ok(dict)
}

fn background_name(background: Background) -> &'static str {
    match background {
        Background::Dark => "dark",
        Background::Light => "light",
    }
}

#[pymethods]
impl Scene {
    /// 形式の名前（sfc・p21・jww・dxf など）
    #[getter]
    fn format(&self) -> &str {
        &self.inner.format
    }

    /// 形式の版（SXF 3.1、JWW 700、DXF AC1032 など）
    #[getter]
    fn version(&self) -> &str {
        &self.inner.version
    }

    /// 図面の座標（values の点・長さ）の長さの単位（millimeter、inch、meter など）。SXF・JWW は millimeter、
    /// DXF・DWG は $INSUNITS（指定がなければ見なした単位）
    #[getter]
    fn unit(&self) -> Option<&'static str> {
        self.inner.unit.map(|unit| unit.name())
    }

    /// 図面の作成時に想定された背景（dark か light）
    #[getter]
    fn background(&self) -> &'static str {
        background_name(self.inner.background)
    }

    /// すべての図形の外接矩形 (min_x, min_y, max_x, max_y)（用紙の mm）
    #[getter]
    fn bounds(&self) -> (f64, f64, f64, f64) {
        let b = &self.inner.bounds;
        (b.min_x, b.min_y, b.max_x, b.max_y)
    }

    /// 用紙（name、size、width、height）。用紙のない図面は None
    #[getter]
    fn sheet<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let Some(sheet) = &self.inner.sheet else {
            return Ok(None);
        };
        let dict = PyDict::new(py);
        dict.set_item("name", &sheet.name)?;
        dict.set_item("size", &sheet.size)?;
        dict.set_item("width", sheet.width)?;
        dict.set_item("height", sheet.height)?;
        Ok(Some(dict))
    }

    /// レイヤ（id、name、visible、group）
    #[getter]
    fn layers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for layer in &self.inner.layers {
            let dict = PyDict::new(py);
            dict.set_item("id", layer.id)?;
            dict.set_item("name", &layer.name)?;
            dict.set_item("visible", layer.visible)?;
            dict.set_item("group", layer.group.as_deref())?;
            list.append(dict)?;
        }
        Ok(list)
    }

    /// 読み取りで省略・代替した内容（level、code、message、count）
    #[getter]
    fn notices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        notice_list(py, &self.inner.notices)
    }

    /// 表題欄・ファイルの情報（項目名と値の組）
    #[getter]
    fn metadata(&self) -> Vec<(String, String)> {
        self.inner.metadata.iter().map(|property| (property.label.clone(), property.value.clone())).collect()
    }

    /// 表題欄・ファイルの情報の、形式によらない key → 値（整形する前の値があればそれ、なければ文字列の値）
    #[getter]
    fn metadata_values<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        property_values(py, &self.inner.metadata)
    }

    /// 選べる表示（DXF・DWG のモデル空間とレイアウトなど）。1 つしかない形式では None
    #[getter]
    fn views(&self) -> Option<Vec<(String, String)>> {
        self.inner.views.as_ref().map(|views| views.iter().map(|view| (view.id.clone(), view.name.clone())).collect())
    }

    /// この図面が描いている表示の id
    #[getter]
    fn view(&self) -> Option<&str> {
        self.inner.view.as_deref()
    }

    /// 図形の数
    fn __len__(&self) -> usize {
        self.inner.entities.len()
    }

    fn __repr__(&self) -> String {
        format!("<zumen.Scene {} {} entities>", self.inner.version, self.inner.entities.len())
    }

    /// TypeScript の sceneToJson と同じ JSON。ブラウザの @zumenjs/core の sceneFromJson で読める
    #[pyo3(signature = (indent = None))]
    fn to_json(&self, py: Python<'_>, indent: Option<usize>) -> PyResult<String> {
        let scene = self.inner.clone();
        py.detach(move || {
            let mut out = Vec::new();
            let result = match indent {
                None => serde_json::to_writer(&mut out, scene.as_ref()),
                Some(width) => {
                    let indent = vec![b' '; width];
                    let formatter = serde_json::ser::PrettyFormatter::with_indent(&indent);
                    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
                    serde::Serialize::serialize(scene.as_ref(), &mut serializer)
                }
            };
            result.map_err(|error| PyValueError::new_err(error.to_string()))?;
            String::from_utf8(out).map_err(|error| PyValueError::new_err(error.to_string()))
        })
    }

    /// 図面を PNG にする。region は用紙の mm の範囲 (min_x, min_y, max_x, max_y) で、省略すると図面全体。
    /// theme は "light" か "dark"（省略すると図面の想定）、hidden_layers は描かないレイヤの id
    /// （省略すると図面の表示指定に従う）
    #[pyo3(signature = (*, dpi = 150.0, region = None, theme = None, line_weights = true, hidden_layers = None, sheet_frame = true))]
    #[allow(clippy::too_many_arguments)]
    fn render_png<'py>(
        &self,
        py: Python<'py>,
        dpi: f64,
        region: Option<(f64, f64, f64, f64)>,
        theme: Option<&str>,
        line_weights: bool,
        hidden_layers: Option<Vec<u32>>,
        sheet_frame: bool,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let options = RenderOptions {
            theme: parse_theme(theme)?,
            line_weights,
            hidden_layers: hidden_layers.map(|ids| ids.into_iter().collect::<HashSet<u32>>()),
            sheet_frame,
            ..RenderOptions::default()
        };
        let region = region.map(|(min_x, min_y, max_x, max_y)| BBox { min_x, min_y, max_x, max_y });
        let scene = self.inner.clone();
        let (png, report) = py
            .detach(move || zumen_render::render_png(&scene, region, dpi, &options))
            .map_err(|error| RenderError::new_err(error.message().to_string()))?;
        warn_report(py, &report)?;
        Ok(PyBytes::new(py, &png))
    }

    /// 図面を SVG にする。region は用紙の mm の範囲で、省略すると用紙（用紙がなければ図面全体）。
    /// 単位は mm で、文字は検索できる text 要素になる。theme・line_weights・hidden_layers・sheet_frame は render_png と同じ
    #[pyo3(signature = (*, region = None, theme = None, line_weights = true, hidden_layers = None, sheet_frame = true))]
    fn render_svg(
        &self,
        py: Python<'_>,
        region: Option<Region>,
        theme: Option<&str>,
        line_weights: bool,
        hidden_layers: Option<Vec<u32>>,
        sheet_frame: bool,
    ) -> PyResult<String> {
        let options = RenderOptions {
            theme: parse_theme(theme)?,
            line_weights,
            hidden_layers: hidden_layers.map(|ids| ids.into_iter().collect::<HashSet<u32>>()),
            sheet_frame,
            ..RenderOptions::default()
        };
        let scene = self.inner.clone();
        let region = region.map(to_bbox);
        let (svg, report) = py
            .detach(move || zumen_render::render_svg(&scene, region, &options))
            .map_err(|error| RenderError::new_err(error.message().to_string()))?;
        warn_report(py, &report)?;
        Ok(svg)
    }

    /// 図面を 1 ページの PDF にする。ページは用紙の大きさ（region を指定すればその範囲）で、線の太さは図面のとおり。
    /// 文字は同梱の BIZ UDゴシックの使う字形だけを埋め込み、検索・コピーできる。引数は render_svg と同じ
    #[pyo3(signature = (*, region = None, theme = None, line_weights = true, hidden_layers = None, sheet_frame = true))]
    fn render_pdf<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        theme: Option<&str>,
        line_weights: bool,
        hidden_layers: Option<Vec<u32>>,
        sheet_frame: bool,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let options = RenderOptions {
            theme: parse_theme(theme)?,
            line_weights,
            hidden_layers: hidden_layers.map(|ids| ids.into_iter().collect::<HashSet<u32>>()),
            sheet_frame,
            ..RenderOptions::default()
        };
        let scene = self.inner.clone();
        let region = region.map(to_bbox);
        let (pdf, report) = py
            .detach(move || zumen_render::render_pdf(&scene, region, &options))
            .map_err(|error| RenderError::new_err(error.message().to_string()))?;
        warn_report(py, &report)?;
        Ok(PyBytes::new(py, &pdf))
    }

    /// 図面を PNG にし、画像の px と用紙の mm の変換と、描いた図形の位置を合わせて返す（`Rendering`）。
    /// 大きさは dpi か、size=(幅, 高さ) の px に余白 margin px を残して収めるかで決める。
    /// region・theme・line_weights・hidden_layers・sheet_frame は render_png と同じ
    #[pyo3(signature = (*, dpi = 150.0, size = None, margin = 0.0, region = None, theme = None, line_weights = true, hidden_layers = None, sheet_frame = true))]
    #[allow(clippy::too_many_arguments)]
    fn render(
        &self,
        py: Python<'_>,
        dpi: f64,
        size: Option<(u32, u32)>,
        margin: f64,
        region: Option<Region>,
        theme: Option<&str>,
        line_weights: bool,
        hidden_layers: Option<Vec<u32>>,
        sheet_frame: bool,
    ) -> PyResult<Rendering> {
        let options = RenderOptions {
            theme: parse_theme(theme)?,
            line_weights,
            hidden_layers: hidden_layers.map(|ids| ids.into_iter().collect::<HashSet<u32>>()),
            sheet_frame,
            ..RenderOptions::default()
        };
        let image_size = match size {
            Some((width, height)) => ImageSize::Fit { width, height, margin },
            None => ImageSize::Dpi(dpi),
        };
        let scene = self.inner.clone();
        let region = region.map(to_bbox);
        let render_options = options.clone();
        let image = py
            .detach(move || zumen_render::render_image(&scene, region, image_size, &render_options))
            .map_err(|error| RenderError::new_err(error.message().to_string()))?;
        warn_report(py, &image.report)?;
        Ok(Rendering {
            scene: self.inner.clone(),
            png: image.png,
            view: image.view,
            width: image.width,
            height: image.height,
            options,
        })
    }

    /// 文字を持つ図形の文字（id、type、text、x、y、height、angle、bbox、layers）を描く順に返す。
    /// text は同じ行の部分をつなげ、行の変わり目に改行を入れた文字列。x・y は最初の部分の文字範囲の左下、
    /// height は文字の高さ、angle は文字の向き（度）、bbox は文字範囲の外接矩形で、どれも用紙の mm。
    /// region を指定すると外接矩形がその範囲と重なる図形だけを、hidden_layers を指定するとそのレイヤを除いて返す
    /// （省略すると図面の表示指定に従い、[] ならすべてのレイヤ）
    #[pyo3(signature = (*, region = None, hidden_layers = None))]
    fn texts<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
    ) -> PyResult<Bound<'py, PyList>> {
        let scene = self.inner.as_ref();
        let list = PyList::empty(py);
        for (entity, text) in scene_texts(scene, &filter(region, hidden_layers)) {
            list.append(text_dict(py, scene, entity, &text)?)?;
        }
        Ok(list)
    }

    /// 図形（id、kind、type、source、layers、bbox、text、properties、values、placement）を描く順に返す。
    /// properties は図面の形式が持つ属性（項目名と値の組）、values は属性の key から値（整形する前の値があればそれ、なければ文字列）への dict、text は文字を持つ図形の文字（なければ None）、
    /// placement は部分図・部品・ブロックの中の図形の配置（name、scale_x、scale_y、inverse）。scale_x・scale_y は元の座標
    /// 1 mm が用紙で何 mm になるか、inverse は用紙の座標から元の座標（実寸の mm）への変換 (a, b, c, d, e, f)
    /// （x' = a x + c y + e、y' = b x + d y + f）。region と hidden_layers は texts と同じ。
    /// geometry を True にすると、描いた形（geometry）を描く順に加える。形は shape（polyline、segments、fill、hatch、
    /// point、text）と用紙の mm の座標（polyline は points、segments は 2 点の組の segments、fill と hatch は輪の rings、
    /// point と text は point）を持ち、real に同じ座標を配置の元の座標（実寸の mm）で持つ（配置の中にない図形は None）。
    /// 曲線は描いた折線で、中心や半径は properties と values にある
    #[pyo3(signature = (*, region = None, hidden_layers = None, geometry = false))]
    fn entities<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
        geometry: bool,
    ) -> PyResult<Bound<'py, PyList>> {
        let scene = self.inner.as_ref();
        let list = PyList::empty(py);
        for entity in select_entities(scene, &filter(region, hidden_layers)) {
            list.append(entity_dict(py, scene, entity, geometry)?)?;
        }
        Ok(list)
    }

    /// 表題欄の項目（number、revision、title、project、scale、date）。見つからない項目は None。
    /// 図面番号は「図面番号」「図番」「DWG NO.」などの項目名の右か下の文字、または属性のタグ（DWGNO など）から推定する。
    /// fields は見つけたすべての候補（key、value、label、source、entity、label_entity、x、y）を項目ごとに確からしい順に
    /// 並べたもの。source は attribute（属性のタグ）、label（項目名の文字の右か下）、inline（項目名と値が 1 つの文字）、
    /// sheet（用紙の名前）、metadata（図面の情報）
    fn title_block<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let fields = title_fields(&self.inner);
        let dict = PyDict::new(py);
        for key in TitleKey::ALL {
            let value = fields.iter().find(|field| field.key == key).map(|field| field.value.as_str());
            dict.set_item(key.name(), value)?;
        }
        let list = PyList::empty(py);
        for field in &fields {
            let item = PyDict::new(py);
            item.set_item("key", field.key.name())?;
            item.set_item("value", &field.value)?;
            item.set_item("label", &field.label)?;
            item.set_item("source", field.source.name())?;
            item.set_item("entity", field.entity.as_deref())?;
            item.set_item("label_entity", field.label_entity.as_deref())?;
            item.set_item("x", field.x)?;
            item.set_item("y", field.y)?;
            list.append(item)?;
        }
        dict.set_item("fields", list)?;
        Ok(dict)
    }

    /// 線で閉じた領域（部屋など）。描いた線を交点と端点で分けてつなぎ、閉じた面を求める（行き止まりの線は面を作らず、
    /// ほかの線とつながらない内側の輪は穴になる）。縮尺の違う配置の線はつながない。dict は bbox、area（穴を除いた
    /// 面積、用紙の mm²）、outline_area（外形の面積）、perimeter（外形の長さ、用紙の mm）、outline（反時計回りの点）、
    /// holes（時計回りの点の輪）、placement（entities と同じ。配置の外なら None）、real（同じ形と大きさを配置の元の
    /// 座標、実寸の mm と mm² で。配置の外なら None）、boundary（境界を描いた図形の id）、texts（中の文字の id、text、
    /// 文字範囲の中心 x・y）。並びは配置ごとに上から下、左から右。
    /// region と hidden_layers は texts と同じ。layers は境界にする図形のレイヤの id（None ならすべて）、kinds は境界に
    /// する種類（None なら line、polyline、multiline、arc、circle、ellipse、ellipse-arc、spline、clothoid、solid）。
    /// tolerance（用紙の mm）より近い端点は同じ点とし、min_area（用紙の mm²）より狭い領域は返さない
    #[pyo3(signature = (*, region = None, hidden_layers = None, layers = None, kinds = None, tolerance = 0.01, min_area = 0.0))]
    #[allow(clippy::too_many_arguments)]
    fn regions<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
        layers: Option<Vec<u32>>,
        kinds: Option<Vec<String>>,
        tolerance: f64,
        min_area: f64,
    ) -> PyResult<Bound<'py, PyList>> {
        if !(tolerance.is_finite() && tolerance > 0.0) {
            return Err(PyValueError::new_err("tolerance は正の数（用紙の mm）にしてください"));
        }
        let kinds = kinds
            .map(|names| {
                names
                    .iter()
                    .map(|name| {
                        zumen::EntityKind::ALL
                            .into_iter()
                            .find(|kind| kind.name() == name)
                            .ok_or_else(|| PyValueError::new_err(format!("知らない図形の種類です: {name}")))
                    })
                    .collect::<PyResult<Vec<_>>>()
            })
            .transpose()?;
        let options = RegionOptions {
            filter: filter(region, hidden_layers),
            kinds,
            layers: layers.map(|ids| ids.into_iter().collect()),
            tolerance,
            min_area,
        };
        let scene = self.inner.clone();
        let regions = py.detach(move || closed_regions(&scene, &options));
        let list = PyList::empty(py);
        for region in &regions {
            list.append(region_dict(py, region)?)?;
        }
        Ok(list)
    }

    /// 罫線で描いた表（面積表・建具表・図面リスト・表題欄など）。水平と垂直の線だけで閉じた長方形の面をます目とし、
    /// 辺を共有して長方形を隙間なく埋めるます目を 1 つの表にする（斜線は罫線にせず、図枠の内側のように表の大半を
    /// 占めるます目は除く）。dict は bbox、placement（regions と同じ）、row_count・column_count、columns（列の境の
    /// x、左から）、rows（行の境の y、上から）、cells（row、column、row_span、column_span、bbox、text、texts。行の順）、
    /// grid（行ごとの文字。結合したます目の文字は左上に入れる）、boundary（罫線を描いた図形の id）。並びは配置ごとに
    /// 上から下、左から右。region・hidden_layers・layers・tolerance は regions と同じ。min_cells より少ないます目の表と、
    /// empty が False なら文字のない表は返さない
    #[pyo3(signature = (*, region = None, hidden_layers = None, layers = None, tolerance = 0.01, min_cells = 2, empty = false))]
    #[allow(clippy::too_many_arguments)]
    fn tables<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
        layers: Option<Vec<u32>>,
        tolerance: f64,
        min_cells: usize,
        empty: bool,
    ) -> PyResult<Bound<'py, PyList>> {
        if !(tolerance.is_finite() && tolerance > 0.0) {
            return Err(PyValueError::new_err("tolerance は正の数（用紙の mm）にしてください"));
        }
        let options = TableOptions {
            filter: filter(region, hidden_layers),
            layers: layers.map(|ids| ids.into_iter().collect()),
            tolerance,
            min_cells,
            empty,
        };
        let scene = self.inner.clone();
        let tables = py.detach(move || ruled_tables(&scene, &options));
        let list = PyList::empty(py);
        for table in &tables {
            list.append(table_dict(py, table)?)?;
        }
        Ok(list)
    }

    /// 製図の規則（レイヤ名・色・線種・線の太さ・文字の高さ・使えない文字・用紙・尺度）に合うかを調べる。rules は組み込みの
    /// 規則集の名前（mlit-cad-2025）、JSON の規則集の文字列か dict（docs/checks.md）。dict は rule_set、title、passed
    /// （error の規則に合わない値がない）、results（規則ごとの id、kind、level、reference、description、checked、
    /// unchecked、findings。findings は合わない値ごとの value、message、count、entities、layers）
    #[pyo3(signature = (rules = None))]
    fn check<'py>(&self, py: Python<'py>, rules: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyDict>> {
        let text: String = match rules {
            None => "mlit-cad-2025".to_string(),
            Some(rules) => match rules.extract::<String>() {
                Ok(text) => text,
                Err(_) => py.import("json")?.call_method1("dumps", (rules,))?.extract()?,
            },
        };
        let rules = RuleSet::named_or_json(&text).map_err(PyValueError::new_err)?;
        let scene = self.inner.clone();
        let report = py.detach(move || check_scene(&scene, &rules));
        check_dict(py, &report)
    }

    /// id の図形（entities と同じ dict で、geometry も entities と同じ）。ない id なら None
    #[pyo3(signature = (id, *, geometry = false))]
    fn entity<'py>(&self, py: Python<'py>, id: &str, geometry: bool) -> PyResult<Option<Bound<'py, PyDict>>> {
        let scene = self.inner.as_ref();
        let ids = self.ids.get_or_init(|| {
            let mut ids = HashMap::with_capacity(scene.entities.len());
            for (index, entity) in scene.entities.iter().enumerate() {
                ids.entry(entity.id.clone()).or_insert(index);
            }
            ids
        });
        ids.get(id).map(|&index| entity_dict(py, scene, &scene.entities[index], geometry)).transpose()
    }

    /// 図形の表（pyarrow.Table）。列は id、kind、type、source、layers、min_x・min_y・max_x・max_y（用紙の mm）、text、
    /// properties（key と label と value の組の並び）、placement、scale_x、scale_y。region と hidden_layers は texts と同じ。
    /// pyarrow が要る（pip install "zumen[arrow]"）
    #[pyo3(signature = (*, region = None, hidden_layers = None))]
    fn to_arrow<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let pa = py
            .import("pyarrow")
            .map_err(|_| PyImportError::new_err("to_arrow には pyarrow が要ります（pip install \"zumen[arrow]\"）"))?;
        let scene = self.inner.as_ref();
        let entities = select_entities(scene, &filter(region, hidden_layers));
        let mut ids = Vec::with_capacity(entities.len());
        let mut kinds = Vec::with_capacity(entities.len());
        let mut types = Vec::with_capacity(entities.len());
        let mut sources = Vec::with_capacity(entities.len());
        let mut layers = Vec::with_capacity(entities.len());
        let mut boxes: [Vec<f64>; 4] = Default::default();
        let mut texts = Vec::with_capacity(entities.len());
        let properties = PyList::empty(py);
        let mut placements = Vec::with_capacity(entities.len());
        let mut scales_x = Vec::with_capacity(entities.len());
        let mut scales_y = Vec::with_capacity(entities.len());
        for entity in &entities {
            ids.push(entity.id.as_str());
            kinds.push(entity.kind.name());
            types.push(entity.type_name.as_str());
            sources.push(entity.source.as_str());
            layers.push(layer_names(scene, entity));
            let b = &entity.bbox;
            for (column, value) in boxes.iter_mut().zip([b.min_x, b.min_y, b.max_x, b.max_y]) {
                column.push(value);
            }
            texts.push(entity_text(entity).map(|text| text.text));
            let row = PyList::empty(py);
            for property in &entity.properties {
                let item = PyDict::new(py);
                item.set_item("key", property.key)?;
                item.set_item("label", &property.label)?;
                item.set_item("value", &property.value)?;
                row.append(item)?;
            }
            properties.append(row)?;
            placements.push(entity.placement.as_ref().map(|placement| placement.name.as_str()));
            scales_x.push(entity.placement.as_ref().map(|placement| placement.scale_x));
            scales_y.push(entity.placement.as_ref().map(|placement| placement.scale_y));
        }
        let string = pa.call_method0("string")?;
        let float64 = pa.call_method0("float64")?;
        let property = pa.call_method1(
            "struct",
            (vec![("key", string.clone()), ("label", string.clone()), ("value", string.clone())],),
        )?;
        let fields = vec![
            ("id", string.clone()),
            ("kind", string.clone()),
            ("type", string.clone()),
            ("source", string.clone()),
            ("layers", pa.call_method1("list_", (string.clone(),))?),
            ("min_x", float64.clone()),
            ("min_y", float64.clone()),
            ("max_x", float64.clone()),
            ("max_y", float64.clone()),
            ("text", string.clone()),
            ("properties", pa.call_method1("list_", (property,))?),
            ("placement", string.clone()),
            ("scale_x", float64.clone()),
            ("scale_y", float64),
        ];
        let schema = pa.call_method1("schema", (fields,))?;
        let [min_x, min_y, max_x, max_y] = boxes;
        let columns = PyDict::new(py);
        columns.set_item("id", ids)?;
        columns.set_item("kind", kinds)?;
        columns.set_item("type", types)?;
        columns.set_item("source", sources)?;
        columns.set_item("layers", layers)?;
        columns.set_item("min_x", min_x)?;
        columns.set_item("min_y", min_y)?;
        columns.set_item("max_x", max_x)?;
        columns.set_item("max_y", max_y)?;
        columns.set_item("text", texts)?;
        columns.set_item("properties", properties)?;
        columns.set_item("placement", placements)?;
        columns.set_item("scale_x", scales_x)?;
        columns.set_item("scale_y", scales_y)?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("schema", schema)?;
        pa.call_method("table", (columns,), Some(&kwargs))
    }
}

/// PNG にした図面と、画像の px と用紙の mm の変換、描いた図形の位置。Jupyter ではそのまま画像を表示する
#[pyclass(frozen, module = "zumen")]
struct Rendering {
    scene: Arc<zumen::Scene>,
    png: Vec<u8>,
    view: View,
    width: u32,
    height: u32,
    options: RenderOptions,
}

/// 描いた図形の dict（id、kind、type、box、text）。box は画像の px の (left, top, right, bottom)
fn drawn_dict<'py>(py: Python<'py>, entity: &SceneEntity, b: &zumen_render::PixelBox) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("kind", entity.kind.name())?;
    dict.set_item("type", &entity.type_name)?;
    dict.set_item("box", (b.left, b.top, b.right, b.bottom))?;
    dict.set_item("text", entity_text(entity).map(|text| text.text))?;
    Ok(dict)
}

impl Rendering {
    fn drawn(&self) -> Vec<(&SceneEntity, zumen_render::PixelBox)> {
        zumen_render::drawn_entities(&self.scene, &self.view, self.width, self.height, &self.options)
            .into_iter()
            .filter_map(|entity| Some((entity, self.view.pixel_box(&entity.bbox, self.width, self.height)?)))
            .collect()
    }
}

#[pymethods]
impl Rendering {
    /// PNG のバイト列
    #[getter]
    fn png<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.png)
    }

    /// 画像の幅（px）
    #[getter]
    fn width(&self) -> u32 {
        self.width
    }

    /// 画像の高さ（px）
    #[getter]
    fn height(&self) -> u32 {
        self.height
    }

    /// 用紙の 1 mm が何 px か
    #[getter]
    fn scale(&self) -> f64 {
        self.view.scale
    }

    /// 画像に写る用紙の範囲 (min_x, min_y, max_x, max_y)（mm）
    #[getter]
    fn region(&self) -> Region {
        from_bbox(&self.view.visible_region(self.width, self.height))
    }

    /// 用紙の mm を画像の px にする（左上が原点で、Y は下向き）
    fn to_pixel(&self, x: f64, y: f64) -> (f64, f64) {
        self.view.to_pixel(x, y)
    }

    /// 画像の px を用紙の mm にする
    fn to_paper(&self, px: f64, py: f64) -> (f64, f64) {
        self.view.to_paper(px, py)
    }

    /// 描いた図形（id、type、box、text）を描く順に返す。box は外接矩形を画像の px にしたもの
    /// (left, top, right, bottom) で、画像の外は切る
    #[getter]
    fn entities<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for (entity, b) in self.drawn() {
            list.append(drawn_dict(py, entity, &b)?)?;
        }
        Ok(list)
    }

    /// 画像の点 (px, py) を外接矩形が含む図形（entities と同じ dict）を、外接矩形の小さい順に返す。
    /// tolerance は外接矩形を広げる px（線のような細い図形を拾うため）
    #[pyo3(signature = (px, py, *, tolerance = 3.0))]
    fn entities_at<'py>(&self, python: Python<'py>, px: f64, py: f64, tolerance: f64) -> PyResult<Bound<'py, PyList>> {
        let mut hits: Vec<(&SceneEntity, zumen_render::PixelBox)> = self
            .drawn()
            .into_iter()
            .filter(|(_, b)| {
                b.left - tolerance <= px
                    && px <= b.right + tolerance
                    && b.top - tolerance <= py
                    && py <= b.bottom + tolerance
            })
            .collect();
        hits.sort_by(|(_, a), (_, b)| {
            let area = |b: &zumen_render::PixelBox| (b.right - b.left) * (b.bottom - b.top);
            area(a).total_cmp(&area(b))
        });
        let list = PyList::empty(python);
        for (entity, b) in hits {
            list.append(drawn_dict(python, entity, &b)?)?;
        }
        Ok(list)
    }

    /// Jupyter の表示
    fn _repr_png_<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.png)
    }

    fn __repr__(&self) -> String {
        format!("<zumen.Rendering {} × {} px, {:.4} px/mm>", self.width, self.height, self.view.scale)
    }
}

/// notices の dict（level、code、message、count）の list
fn notice_list<'py>(py: Python<'py>, notices: &[zumen::Notice]) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for notice in notices {
        let dict = PyDict::new(py);
        let level = match notice.level {
            zumen::NoticeLevel::Info => "info",
            zumen::NoticeLevel::Warning => "warning",
        };
        dict.set_item("level", level)?;
        dict.set_item("code", &notice.code)?;
        dict.set_item("message", &notice.message)?;
        dict.set_item("count", notice.count)?;
        list.append(dict)?;
    }
    Ok(list)
}

/// 書き直した図面（data）と、書けなかった・形を変えて書いた内容（notices）
#[pyclass(frozen, module = "zumen")]
struct Converted {
    inner: zumen::Converted,
    format: String,
}

#[pymethods]
impl Converted {
    /// 書き直した図面のバイト列
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }

    /// 書いた形式（dxf）
    #[getter]
    fn format(&self) -> &str {
        &self.format
    }

    /// 書けなかった・形を変えて書いた内容（level、code、message、count）
    #[getter]
    fn notices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        notice_list(py, &self.inner.notices)
    }

    /// ファイルに書く
    fn save(&self, py: Python<'_>, path: std::path::PathBuf) -> PyResult<()> {
        py.detach(|| std::fs::write(&path, &self.inner.bytes))
            .map_err(|error| PyOSError::new_err(format!("{}: {error}", path.display())))
    }

    fn __len__(&self) -> usize {
        self.inner.bytes.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "<zumen.Converted {}, {} bytes, {} notices>",
            self.format,
            self.inner.bytes.len(),
            self.inner.notices.len()
        )
    }
}

/// 編集した図面（data）、足したものの id（created）、形を変えて書いた内容（notices）
#[pyclass(frozen, module = "zumen")]
struct Edited {
    inner: zumen::edit::Edited,
}

#[pymethods]
impl Edited {
    /// 編集した図面のバイト列
    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.inner.bytes)
    }

    /// add で足した図形の id と、add-layer で足したレイヤの名前（操作の順）
    #[getter]
    fn created(&self) -> Vec<String> {
        self.inner.created.clone()
    }

    /// 形を変えて書いた・足した内容（level、code、message、count）
    #[getter]
    fn notices<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        notice_list(py, &self.inner.notices)
    }

    /// ファイルに書く
    fn save(&self, py: Python<'_>, path: std::path::PathBuf) -> PyResult<()> {
        py.detach(|| std::fs::write(&path, &self.inner.bytes))
            .map_err(|error| PyOSError::new_err(format!("{}: {error}", path.display())))
    }

    fn __len__(&self) -> usize {
        self.inner.bytes.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "<zumen.Edited {} bytes, {} created, {} notices>",
            self.inner.bytes.len(),
            self.inner.created.len(),
            self.inner.notices.len()
        )
    }
}

/// 図面をその場で編集する（docs/decisions/0012）。operations は操作の list（または JSON の文字列）。
/// 形式・版・文字コードと、操作が名指ししないものは元のまま。1 つでも通らない操作があれば EditError を出し、何も書かない
#[pyfunction]
#[pyo3(signature = (data, operations, *, format = None, max_bytes = None))]
fn edit(
    py: Python<'_>,
    data: &[u8],
    operations: &Bound<'_, PyAny>,
    format: Option<String>,
    max_bytes: Option<u64>,
) -> PyResult<Edited> {
    let operations = edit_operations(py, operations)?;
    let limits = zumen::ReadLimits { max_bytes, ..zumen::ReadLimits::default() };
    let options = ReadDrawingOptions { format, limits, ..ReadDrawingOptions::default() };
    let edited = py.detach(|| zumen::edit(data, &operations, &options)).map_err(|error| edit_error(py, error))?;
    Ok(Edited { inner: edited })
}

/// 新しい図面を作り、操作を当てる。to は形式（dxf・dwg・jww・sfc・p21・sfz・p2z）、units は長さの単位
/// （mm・cm・m・inch・foot・unitless。JWW と SXF は mm だけ）
#[pyfunction]
#[pyo3(signature = (to, operations = None, *, units = "mm".to_string()))]
fn create(py: Python<'_>, to: String, operations: Option<&Bound<'_, PyAny>>, units: String) -> PyResult<Edited> {
    let operations = match operations {
        Some(operations) => edit_operations(py, operations)?,
        None => Vec::new(),
    };
    let created = py.detach(|| zumen::create(&to, &units, &operations)).map_err(|error| edit_error(py, error))?;
    Ok(Edited { inner: created })
}

/// 図面の形式を中身から判定する（sfc・p21・jww・dxf など）。読めない形式なら None
#[pyfunction]
fn detect(data: &[u8]) -> Option<&'static str> {
    zumen::detect(data)
}

/// 図面のバイト列を読み、表示用の図面にする。format は分かっている場合の形式、view は描く表示の id。
/// max_bytes（圧縮した SXF は展開した後）・max_entities・timeout（秒）を超えたら途中でやめ、LimitError を出す
#[pyfunction]
#[pyo3(signature = (data, *, format = None, view = None, max_bytes = None, max_entities = None, timeout = None))]
#[allow(clippy::too_many_arguments)]
fn read(
    py: Python<'_>,
    data: &[u8],
    format: Option<String>,
    view: Option<String>,
    max_bytes: Option<u64>,
    max_entities: Option<u64>,
    timeout: Option<f64>,
) -> PyResult<Scene> {
    let max_duration = timeout
        .map(|seconds| {
            std::time::Duration::try_from_secs_f64(seconds)
                .map_err(|_| PyValueError::new_err(format!("timeout は 0 以上の秒数です: {seconds}")))
        })
        .transpose()?;
    let limits = zumen::ReadLimits { max_bytes, max_entities, max_duration };
    let options = ReadDrawingOptions { format, view, limits };
    let scene = py.detach(|| zumen::read(data, &options)).map_err(|error| read_error(py, error))?;
    Ok(Scene::new(scene))
}

/// 2 つの版の比較（compare の結果）。図形の dict は entities と同じ形で、それぞれの版の座標を持つ
#[pyclass(frozen, module = "zumen")]
struct Comparison {
    before: Arc<zumen::Scene>,
    after: Arc<zumen::Scene>,
    diff: SceneDiff,
    /// 重ねた図（scene で初めて使うときに作る）
    overlay: OnceLock<Py<Scene>>,
}

#[pymethods]
impl Comparison {
    /// 前の版にだけある図形
    #[getter]
    fn removed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for &i in &self.diff.removed {
            list.append(entity_dict(py, &self.before, &self.before.entities[i], false)?)?;
        }
        Ok(list)
    }

    /// 後の版にだけある図形
    #[getter]
    fn added<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for &i in &self.diff.added {
            list.append(entity_dict(py, &self.after, &self.after.entities[i], false)?)?;
        }
        Ok(list)
    }

    /// 変わったとみなした図形の組（before、after）
    #[getter]
    fn changed<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for &(a, b) in &self.diff.changed {
            let pair = PyDict::new(py);
            pair.set_item("before", entity_dict(py, &self.before, &self.before.entities[a], false)?)?;
            pair.set_item("after", entity_dict(py, &self.after, &self.after.entities[b], false)?)?;
            list.append(pair)?;
        }
        Ok(list)
    }

    /// 変わらなかった図形の数
    #[getter]
    fn unchanged(&self) -> usize {
        self.diff.unchanged.len()
    }

    /// 後の版を前の版に合わせるために、後の版の座標から引いたずれ (dx, dy)（用紙の mm）
    #[getter]
    fn offset(&self) -> (f64, f64) {
        self.diff.offset
    }

    /// 変わった場所（bbox、removed、added、changed）。前の版の用紙の mm で、変わった図形の多い順
    #[getter]
    fn regions<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for region in &self.diff.regions {
            let dict = PyDict::new(py);
            dict.set_item("bbox", from_bbox(&region.bbox))?;
            dict.set_item("removed", region.removed)?;
            dict.set_item("added", region.added)?;
            dict.set_item("changed", region.changed)?;
            list.append(dict)?;
        }
        Ok(list)
    }

    /// 重ねた図。変わらない図形は灰、前の版だけの図形（削除・変更前）は赤、後の版だけの図形（追加・変更後）は青。
    /// render・render_png で描け、to_json でブラウザの表示に渡せる
    #[getter]
    fn scene(&self, py: Python<'_>) -> PyResult<Py<Scene>> {
        if let Some(scene) = self.overlay.get() {
            return Ok(scene.clone_ref(py));
        }
        let (before, after) = (self.before.clone(), self.after.clone());
        let diff = &self.diff;
        let overlay = py.detach(|| diff_scene(&before, &after, diff));
        let scene = Py::new(py, Scene::new(overlay))?;
        Ok(self.overlay.get_or_init(|| scene).clone_ref(py))
    }

    /// 違いの数（削除・追加・変更）
    fn __len__(&self) -> usize {
        self.diff.removed.len() + self.diff.added.len() + self.diff.changed.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "<zumen.Comparison unchanged {}, removed {}, added {}, changed {}>",
            self.diff.unchanged.len(),
            self.diff.removed.len(),
            self.diff.added.len(),
            self.diff.changed.len()
        )
    }
}

/// 2 つの版の図面を比べる。図形の id は使わず、種類・形・文字で照合する。
/// tolerance は同じ位置とみなす差（用紙の mm）、style を真にすると色・線の太さ・線種・レイヤの違いも変更とみなす。
/// all_layers を真にすると描かないレイヤも比べ、align を真にすると同じ文字の位置から全体のずれを見つけて合わせる
#[pyfunction]
#[pyo3(signature = (before, after, *, tolerance = 0.01, style = false, all_layers = false, align = true))]
fn compare(
    py: Python<'_>,
    before: &Scene,
    after: &Scene,
    tolerance: f64,
    style: bool,
    all_layers: bool,
    align: bool,
) -> PyResult<Comparison> {
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(PyValueError::new_err("tolerance は正の数です"));
    }
    let options = CompareOptions { tolerance, style, all_layers, align };
    let (a, b) = (before.inner.clone(), after.inner.clone());
    let diff = py.detach(|| compare_scenes(&a, &b, &options));
    Ok(Comparison { before: a, after: b, diff, overlay: OnceLock::new() })
}

/// 図面を読み、to の形式（dxf・dwg・jww・sfc・p21・sfz・p2z）に書き直す。どの形式の図面も書き直せる（SXF・JWW は AutoCAD の図面のモデルに写す）。
/// format は分かっている場合の読む形式
#[pyfunction]
#[pyo3(signature = (data, to, *, format = None, version = None))]
fn convert(
    py: Python<'_>,
    data: &[u8],
    to: String,
    format: Option<String>,
    version: Option<String>,
) -> PyResult<Converted> {
    let options = ReadDrawingOptions { format, ..ReadDrawingOptions::default() };
    let write = zumen::WriteOptions { version };
    let converted = py
        .detach(|| zumen::convert_with(data, &to, &options, &write))
        .map_err(|error| ReadError::new_err(error.message().to_string()))?;
    Ok(Converted { inner: converted, format: to })
}

/// 図面の仕様（docs/decisions/0018、docs/drafting.md）から、用紙・縮尺のあるビュー・寸法・ハッチング・表・表題欄の
/// そろった図面を描き、to の形式（dxf・dwg・jww・sfc・p21・sfz・p2z）で書く。spec は dict（JSON にできる値）か JSON の
/// 文字列。version は書く版、sheet は描く用紙の名前（JWW と SXF は用紙を 1 枚しか持たない）
#[pyfunction]
#[pyo3(signature = (spec, to = "dxf".to_string(), *, version = None, sheet = None))]
fn draft(
    py: Python<'_>,
    spec: &Bound<'_, PyAny>,
    to: String,
    version: Option<String>,
    sheet: Option<String>,
) -> PyResult<Converted> {
    let text: String = match spec.extract::<String>() {
        Ok(text) => text,
        Err(_) => py.import("json")?.call_method1("dumps", (spec,))?.extract()?,
    };
    let options = zumen::DraftOptions { write: zumen::WriteOptions { version }, sheet };
    let drafted = py.detach(|| zumen::draft(&text, &to, &options)).map_err(|error| draft_error(py, &error))?;
    Ok(Converted { inner: drafted, format: to })
}

/// 図面の仕様を、受け取った DXF・DWG・JWW・SXF の図面（テンプレート）にその場で描く（zumen::draft_into）。元の図形・
/// ハンドル・拡張データは残り、形式と版も元のまま。sheet は描く用紙の名前、format は分かっている場合の図面の形式
#[pyfunction]
#[pyo3(signature = (spec, data, *, sheet = None, format = None, max_bytes = None))]
fn draft_into(
    py: Python<'_>,
    spec: &Bound<'_, PyAny>,
    data: &[u8],
    sheet: Option<String>,
    format: Option<String>,
    max_bytes: Option<u64>,
) -> PyResult<Converted> {
    let text: String = match spec.extract::<String>() {
        Ok(text) => text,
        Err(_) => py.import("json")?.call_method1("dumps", (spec,))?.extract()?,
    };
    let mut read = ReadDrawingOptions { format, ..ReadDrawingOptions::default() };
    read.limits.max_bytes = max_bytes;
    let options = zumen::DraftIntoOptions { sheet, read };
    let drafted = py.detach(|| zumen::draft_into(data, &text, &options)).map_err(|error| draft_error(py, &error))?;
    let to = zumen::detect(&drafted.bytes).unwrap_or("dxf").to_string();
    Ok(Converted { inner: drafted, format: to })
}

/// 仕様の誤りを、code と path のある DraftError にする
fn draft_error(py: Python<'_>, error: &zumen::draft::DraftError) -> PyErr {
    let err = DraftError::new_err(error.to_string());
    let value = err.value(py);
    if let Err(failure) = value.setattr("code", error.code).and_then(|()| value.setattr("path", error.path.clone())) {
        return failure;
    }
    err
}

/// 図面の仕様の JSON Schema（JSON の文字列）
#[pyfunction]
fn draft_schema() -> &'static str {
    zumen::draft::SCHEMA
}

#[pymodule]
fn _zumen(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ReadError", m.py().get_type::<ReadError>())?;
    m.add("LimitError", m.py().get_type::<LimitError>())?;
    m.add("RenderError", m.py().get_type::<RenderError>())?;
    m.add("EditError", m.py().get_type::<EditError>())?;
    m.add("DraftError", m.py().get_type::<DraftError>())?;
    m.add_class::<Scene>()?;
    m.add_class::<Rendering>()?;
    m.add_class::<Comparison>()?;
    m.add_function(wrap_pyfunction!(compare, m)?)?;
    m.add_class::<Converted>()?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_class::<Edited>()?;
    m.add_function(wrap_pyfunction!(edit, m)?)?;
    m.add_function(wrap_pyfunction!(create, m)?)?;
    m.add_function(wrap_pyfunction!(draft, m)?)?;
    m.add_function(wrap_pyfunction!(draft_into, m)?)?;
    m.add_function(wrap_pyfunction!(draft_schema, m)?)?;
    Ok(())
}
