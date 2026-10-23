//! zumen の PDF の読み取りの WebAssembly の結合部。ブラウザ・Node.js で PDF の図面を読み、Scene の JSON を返す。
//! hayro-interpret と、それが埋め込むフォント・CMap・画像の復号で大きいので、ほかの形式のモジュール（bindings/wasm）とは
//! 分け、PDF を初めて読むときにだけ読み込む（決定 0030）。関数は bindings/wasm と同じ形で、@zumenjs/reader が形式で選ぶ。
//! scripts/wasm.mjs が packages/reader/wasm に作る。

use wasm_bindgen::prelude::*;
use zumen_core::{ReadLimits, ReadOptions, read_drawing_with_limits};

/// 中身が PDF なら "pdf"。そうでなければ undefined
#[wasm_bindgen]
pub fn detect(bytes: &[u8]) -> Option<String> {
    zumen_pdf::is_pdf(bytes).then(|| "pdf".to_string())
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

impl From<zumen_core::ReadError> for ReadFailure {
    fn from(error: zumen_core::ReadError) -> Self {
        Self { message: error.message().to_string(), limit: error.exceeded().map(|limit| limit.name().to_string()) }
    }
}

/// PDF を読み、Scene の JSON を返す。format は分かっている場合の形式（pdf）、view は描くページの番号（"1" から）。
/// max_bytes と max_entities は読み取りの上限で、超えたら ReadFailure を投げる（bindings/wasm の readSceneJson と同じ）
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
    let scene = read_drawing_with_limits(bytes, &[&zumen_pdf::PDF], format.as_deref(), &ReadOptions { view }, &limits)?;
    serde_json::to_string(&scene).map_err(|error| ReadFailure { message: error.to_string(), limit: None })
}
