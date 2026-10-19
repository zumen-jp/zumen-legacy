//! zumen の WebAssembly の結合部。ブラウザ・Node.js で図面を読み、Scene の JSON を返す。
//! JSON は @zumenjs/core の sceneFromJson で Scene に戻し、DrawingViewer に渡せる。
//! scripts/wasm.mjs が packages/dwg/wasm に作り、@zumenjs/dwg がブラウザで DWG を読むのに使う。

use wasm_bindgen::prelude::*;
use zumen::ReadDrawingOptions;

/// 図面の形式を中身から判定する（sfc・p21・jww・dxf など）。読めない形式なら undefined
#[wasm_bindgen]
pub fn detect(bytes: &[u8]) -> Option<String> {
    zumen::detect(bytes).map(String::from)
}

/// 図面を読み、Scene の JSON を返す。format は分かっている場合の形式、view は描く表示の id
#[wasm_bindgen(js_name = readSceneJson)]
pub fn read_scene_json(bytes: &[u8], format: Option<String>, view: Option<String>) -> Result<String, JsError> {
    let scene =
        zumen::read(bytes, &ReadDrawingOptions { format, view }).map_err(|error| JsError::new(error.message()))?;
    serde_json::to_string(&scene).map_err(|error| JsError::new(&error.to_string()))
}
