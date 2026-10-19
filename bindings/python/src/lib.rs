//! zumen の Python の結合部。図面を読み（`read`）、表示用モデル（`Scene`）を JSON や PNG にする。
//! 読み取りと描画の間は GIL を外す（FastAPI のスレッドなどで並行に動く）。

use pyo3::exceptions::{PyUserWarning, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::HashSet;
use std::sync::Arc;
use zumen::{BBox, Background, ReadDrawingOptions};
use zumen_render::RenderOptions;

pyo3::create_exception!(zumen, ReadError, PyValueError, "図面を読めなかった（形式が違う、壊れているなど）");
pyo3::create_exception!(zumen, RenderError, PyValueError, "図面を描けなかった（範囲が空、画像が大きすぎるなど）");

/// 形式に依存しない表示用の図面。座標は用紙の mm で、Y 軸は上向き
#[pyclass(frozen, module = "zumen")]
struct Scene {
    inner: Arc<zumen::Scene>,
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
        let theme = match theme {
            None => None,
            Some("light") => Some(Background::Light),
            Some("dark") => Some(Background::Dark),
            Some(other) => return Err(PyValueError::new_err(format!("theme は light か dark です: {other}"))),
        };
        let options = RenderOptions {
            theme,
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
        if !report.missing_glyphs.is_empty() {
            let characters: String = report.missing_glyphs.iter().collect();
            let message = format!("同梱のフォントに字形がない文字を描いていません: {characters}");
            PyErr::warn(py, &py.get_type::<PyUserWarning>(), &std::ffi::CString::new(message)?, 1)?;
        }
        Ok(PyBytes::new(py, &png))
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
    Ok(Scene { inner: Arc::new(scene) })
}

#[pymodule]
fn _zumen(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ReadError", m.py().get_type::<ReadError>())?;
    m.add("RenderError", m.py().get_type::<RenderError>())?;
    m.add_class::<Scene>()?;
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    Ok(())
}
