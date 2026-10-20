//! zumen の WebAssembly の結合部。ブラウザ・Node.js で図面を読み、Scene の JSON を返す。
//! JSON は @zumenjs/core の sceneFromJson で Scene に戻し、DrawingViewer に渡せる。
//! scripts/wasm.mjs が packages/reader/wasm に作り、@zumenjs/reader がブラウザでどの形式も読むのに使う。

use wasm_bindgen::prelude::*;
use zumen::{ReadDrawingOptions, ReadLimits};

/// 図面の形式を中身から判定する（sfc・p21・jww・dxf など）。読めない形式なら undefined
#[wasm_bindgen]
pub fn detect(bytes: &[u8]) -> Option<String> {
    zumen::detect(bytes).map(String::from)
}

/// 読めなかったときに投げる値。message は利用者に見せる文、limit は超えた上限の種類（bytes・entities）
#[wasm_bindgen(js_name = ReadFailure)]
pub struct ReadFailure {
    message: String,
    limit: Option<String>,
}

#[wasm_bindgen(js_class = ReadFailure)]
impl ReadFailure {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// 上限を超えたための失敗なら、その上限の名前
    #[wasm_bindgen(getter)]
    pub fn limit(&self) -> Option<String> {
        self.limit.clone()
    }
}

impl From<zumen::ReadError> for ReadFailure {
    fn from(error: zumen::ReadError) -> Self {
        Self { message: error.message().to_string(), limit: error.exceeded().map(|limit| limit.name().to_string()) }
    }
}

/// 図面を読み、Scene の JSON を返す。format は分かっている場合の形式、view は描く表示の id。
/// max_bytes（圧縮した SXF は展開した後）と max_entities は読み取りの上限で、超えたら ReadFailure を投げる。
/// 時間の上限は WebAssembly では測れないので、Web Worker で読んで止める（@zumenjs/reader の ReaderWorker）
#[wasm_bindgen(js_name = readSceneJson)]
pub fn read_scene_json(
    bytes: &[u8],
    format: Option<String>,
    view: Option<String>,
    max_bytes: Option<f64>,
    max_entities: Option<f64>,
) -> Result<String, ReadFailure> {
    // JavaScript の数は f64。負の数と NaN は 0、大きすぎる数は u64 の最大に丸める（as の飽和）
    let limits = ReadLimits {
        max_bytes: max_bytes.map(|max| max as u64),
        max_entities: max_entities.map(|max| max as u64),
        max_duration: None,
    };
    let scene = zumen::read(bytes, &ReadDrawingOptions { format, view, limits })?;
    serde_json::to_string(&scene).map_err(|error| ReadFailure { message: error.to_string(), limit: None })
}
