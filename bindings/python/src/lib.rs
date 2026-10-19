//! zumen の Python の結合部。図面を読み（`read`）、表示用モデル（`Scene`）を JSON や PNG にし、文字と図形を取り出す。
//! 読み取りと描画の間は GIL を外す（FastAPI のスレッドなどで並行に動く）。

use pyo3::exceptions::{PyImportError, PyUserWarning, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use zumen::extract::{EntityFilter, EntityText, entity_text, layer_names, scene_texts, select_entities};
use zumen::{BBox, Background, ReadDrawingOptions, SceneEntity};
use zumen_render::{ImageSize, RenderOptions, View};

pyo3::create_exception!(zumen, ReadError, PyValueError, "図面を読めなかった（形式が違う、壊れているなど）");
pyo3::create_exception!(zumen, RenderError, PyValueError, "図面を描けなかった（範囲が空、画像が大きすぎるなど）");

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

/// 同梱のフォントに字形がなかった文字を、Python の警告にする
fn warn_missing_glyphs(py: Python<'_>, report: &zumen_render::RenderReport) -> PyResult<()> {
    if report.missing_glyphs.is_empty() {
        return Ok(());
    }
    let characters: String = report.missing_glyphs.iter().collect();
    let message = format!("同梱のフォントに字形がない文字を描いていません: {characters}");
    PyErr::warn(py, &py.get_type::<PyUserWarning>(), &std::ffi::CString::new(message)?, 1)
}

/// 図形の dict（id、type、source、layers、bbox、text、properties、placement）
fn entity_dict<'py>(py: Python<'py>, scene: &zumen::Scene, entity: &SceneEntity) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("type", &entity.kind)?;
    dict.set_item("source", &entity.source)?;
    dict.set_item("layers", layer_names(scene, entity))?;
    dict.set_item("bbox", from_bbox(&entity.bbox))?;
    dict.set_item("text", entity_text(entity).map(|text| text.text))?;
    let properties: Vec<(&str, &str)> =
        entity.properties.iter().map(|property| (property.label.as_str(), property.value.as_str())).collect();
    dict.set_item("properties", properties)?;
    let placement = match &entity.placement {
        Some(placement) => {
            let item = PyDict::new(py);
            item.set_item("name", &placement.name)?;
            item.set_item("scale_x", placement.scale_x)?;
            item.set_item("scale_y", placement.scale_y)?;
            let [a, b, c, d, e, f] = placement.inverse;
            item.set_item("inverse", (a, b, c, d, e, f))?;
            Some(item)
        }
        None => None,
    };
    dict.set_item("placement", placement)?;
    Ok(dict)
}

/// 文字の dict（id、type、text、x、y、height、angle、bbox、layers）
fn text_dict<'py>(
    py: Python<'py>,
    scene: &zumen::Scene,
    entity: &SceneEntity,
    text: &EntityText,
) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("type", &entity.kind)?;
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
        let list = PyList::empty(py);
        for notice in &self.inner.notices {
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

    /// 表題欄・ファイルの情報（項目名と値の組）
    #[getter]
    fn metadata(&self) -> Vec<(String, String)> {
        self.inner.metadata.iter().map(|property| (property.label.clone(), property.value.clone())).collect()
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
        warn_missing_glyphs(py, &report)?;
        Ok(PyBytes::new(py, &png))
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
        warn_missing_glyphs(py, &image.report)?;
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

    /// 図形（id、type、source、layers、bbox、text、properties、placement）を描く順に返す。
    /// properties は図面の形式が持つ属性（項目名と値の組）、text は文字を持つ図形の文字（なければ None）、
    /// placement は部分図・部品・ブロックの中の図形の配置（name、scale_x、scale_y、inverse）。scale_x・scale_y は元の座標
    /// 1 mm が用紙で何 mm になるか、inverse は用紙の座標から元の座標（実寸の mm）への変換 (a, b, c, d, e, f)
    /// （x' = a x + c y + e、y' = b x + d y + f）。region と hidden_layers は texts と同じ
    #[pyo3(signature = (*, region = None, hidden_layers = None))]
    fn entities<'py>(
        &self,
        py: Python<'py>,
        region: Option<Region>,
        hidden_layers: Option<Vec<u32>>,
    ) -> PyResult<Bound<'py, PyList>> {
        let scene = self.inner.as_ref();
        let list = PyList::empty(py);
        for entity in select_entities(scene, &filter(region, hidden_layers)) {
            list.append(entity_dict(py, scene, entity)?)?;
        }
        Ok(list)
    }

    /// id の図形（entities と同じ dict）。ない id なら None
    fn entity<'py>(&self, py: Python<'py>, id: &str) -> PyResult<Option<Bound<'py, PyDict>>> {
        let scene = self.inner.as_ref();
        let ids = self.ids.get_or_init(|| {
            let mut ids = HashMap::with_capacity(scene.entities.len());
            for (index, entity) in scene.entities.iter().enumerate() {
                ids.entry(entity.id.clone()).or_insert(index);
            }
            ids
        });
        ids.get(id).map(|&index| entity_dict(py, scene, &scene.entities[index])).transpose()
    }

    /// 図形の表（pyarrow.Table）。列は id、type、source、layers、min_x・min_y・max_x・max_y（用紙の mm）、text、
    /// properties（label と value の組の並び）、placement、scale_x、scale_y。region と hidden_layers は texts と同じ。
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
            types.push(entity.kind.as_str());
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
        let property = pa.call_method1("struct", (vec![("label", string.clone()), ("value", string.clone())],))?;
        let fields = vec![
            ("id", string.clone()),
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

/// 描いた図形の dict（id、type、box、text）。box は画像の px の (left, top, right, bottom)
fn drawn_dict<'py>(py: Python<'py>, entity: &SceneEntity, b: &zumen_render::PixelBox) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("id", &entity.id)?;
    dict.set_item("type", &entity.kind)?;
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

/// 図面の形式を中身から判定する（sfc・p21・jww・dxf など）。読めない形式なら None
#[pyfunction]
fn detect(data: &[u8]) -> Option<&'static str> {
    zumen::detect(data)
}

/// 図面のバイト列を読み、表示用の図面にする。format は分かっている場合の形式、view は描く表示の id
#[pyfunction]
#[pyo3(signature = (data, *, format = None, view = None))]
fn read(py: Python<'_>, data: &[u8], format: Option<String>, view: Option<String>) -> PyResult<Scene> {
    let options = ReadDrawingOptions { format, view };
    let scene =
        py.detach(|| zumen::read(data, &options)).map_err(|error| ReadError::new_err(error.message().to_string()))?;
    Ok(Scene::new(scene))
}

#[pymodule]
fn _zumen(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ReadError", m.py().get_type::<ReadError>())?;
    m.add("RenderError", m.py().get_type::<RenderError>())?;
    m.add_class::<Scene>()?;
    m.add_class::<Rendering>()?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    Ok(())
}
