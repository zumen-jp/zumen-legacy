//! SVG と PDF に共通の描き方。座標は用紙の mm のまま書き、ページの左下を原点にする（大きな座標でも
//! 32 ビットの浮動小数点で位置がずれないように、ずらしは 64 ビットで行う）。

use crate::RenderError;
use crate::RenderOptions;
use zumen_core::extract::{EntityFilter, select_entities};
use zumen_core::{BBox, Scene, SceneEntity};

/// 画面の 1 px（96 dpi）の mm。SVG の細線と、点の大きさに使う
pub(crate) const PX_MM: f64 = 25.4 / 96.0;

/// 点（SXF の点マーカ）の一辺。PNG と同じ 3 px
pub(crate) const DOT_MM: f64 = 3.0 * PX_MM;

/// 描くページ。範囲を指定しなければ用紙、用紙がなければ図面の範囲
pub(crate) struct Page {
    pub region: BBox,
    /// 用紙の枠を描くか（ページが用紙そのものなら描かない）
    pub frame: bool,
}

impl Page {
    pub(crate) fn new(scene: &Scene, region: Option<BBox>, options: &RenderOptions) -> Result<Page, RenderError> {
        let sheet =
            scene.sheet.as_ref().map(|sheet| BBox { min_x: 0.0, min_y: 0.0, max_x: sheet.width, max_y: sheet.height });
        let region = match region.or(sheet) {
            Some(region) => region,
            None => scene.bounds,
        };
        let width = region.max_x - region.min_x;
        let height = region.max_y - region.min_y;
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            return Err(RenderError::new("描く範囲がありません"));
        }
        let frame = options.sheet_frame && sheet.is_some_and(|sheet| sheet != region);
        Ok(Page { region, frame })
    }

    pub(crate) fn width(&self) -> f64 {
        self.region.max_x - self.region.min_x
    }

    pub(crate) fn height(&self) -> f64 {
        self.region.max_y - self.region.min_y
    }

    /// 用紙の mm をページの mm（左下が原点）にする
    pub(crate) fn local(&self, x: f64, y: f64) -> (f64, f64) {
        (x - self.region.min_x, y - self.region.min_y)
    }

    /// 描く図形（表示するレイヤにあり、ページと重なるもの。underlay が見せている図形は除く）
    pub(crate) fn entities<'a>(&self, scene: &'a Scene, options: &RenderOptions) -> Vec<&'a SceneEntity> {
        select_entities(
            scene,
            &EntityFilter { region: Some(self.region), hidden_layers: options.hidden_layers.clone() },
        )
        .into_iter()
        .filter(|entity| entity.drawn)
        .collect()
    }

    /// 用紙の枠（ページの mm の 4 隅）
    pub(crate) fn sheet_corners(&self, scene: &Scene) -> Option<[(f64, f64); 4]> {
        let sheet = scene.sheet.as_ref()?;
        Some(
            [(0.0, 0.0), (sheet.width, 0.0), (sheet.width, sheet.height), (0.0, sheet.height)]
                .map(|(x, y)| self.local(x, y)),
        )
    }
}

/// 破線の長さ（mm）。負の値・数でない値があれば実線、合計が 0 なら実線
pub(crate) fn dash_lengths(dashes: &[f64]) -> Vec<f64> {
    if dashes.iter().any(|value| !(value.is_finite() && *value >= 0.0)) || dashes.iter().sum::<f64>() <= 0.0 {
        return Vec::new();
    }
    dashes.to_vec()
}
