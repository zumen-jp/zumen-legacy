//! JWW のヘッダ（図形の一覧より前）を、Jw_cad の形式説明（jwdatafmt.txt、Jw_cad 7.02）の並びのとおりに、
//! すべての値を型つきで持つ。読み取り（reader.rs）が使わない値も持ち、書くと元と同じバイト列になる。
//!
//! 版 300 より前の版と、仕様の並びで読めなかったところは、図形の一覧の前までのバイト列（`rest`）のまま持つ。

use crate::archive::{Reader, Result, Writer, unicode};
use zumen_core::ReadError;

/// ヘッダの値の読み書き。欄の並びは `record!` の構造体の宣言の順
pub(crate) trait Field: Sized {
    fn read(reader: &mut Reader<'_>, version: u32) -> Result<Self>;
    fn write(&self, out: &mut Writer, version: u32);
}

impl Field for u32 {
    fn read(reader: &mut Reader<'_>, _: u32) -> Result<Self> {
        reader.u32()
    }
    fn write(&self, out: &mut Writer, _: u32) {
        out.u32(*self);
    }
}

impl Field for i32 {
    fn read(reader: &mut Reader<'_>, _: u32) -> Result<Self> {
        reader.i32()
    }
    fn write(&self, out: &mut Writer, _: u32) {
        out.i32(*self);
    }
}

impl Field for f64 {
    fn read(reader: &mut Reader<'_>, _: u32) -> Result<Self> {
        reader.f64()
    }
    fn write(&self, out: &mut Writer, _: u32) {
        out.f64(*self);
    }
}

/// CString。版 700 以降は UTF-16LE、それより前は Shift_JIS で書く
impl Field for String {
    fn read(reader: &mut Reader<'_>, _: u32) -> Result<Self> {
        reader.cstring()
    }
    fn write(&self, out: &mut Writer, version: u32) {
        out.cstring(self, unicode(version));
    }
}

impl<T: Field + Copy + Default, const N: usize> Field for [T; N] {
    fn read(reader: &mut Reader<'_>, version: u32) -> Result<Self> {
        let mut values = [T::default(); N];
        for value in &mut values {
            *value = T::read(reader, version)?;
        }
        Ok(values)
    }
    fn write(&self, out: &mut Writer, version: u32) {
        for value in self {
            value.write(out, version);
        }
    }
}

/// 決まった件数の表を読む
fn read_table<T: Field>(reader: &mut Reader<'_>, version: u32, count: usize) -> Result<Vec<T>> {
    (0..count).map(|_| T::read(reader, version)).collect()
}

/// 決まった件数の表を書く。足りなければ既定の値で補い、多ければ切る
fn write_table<T: Field + Default>(out: &mut Writer, version: u32, values: &[T], count: usize) {
    for index in 0..count {
        match values.get(index) {
            Some(value) => value.write(out, version),
            None => T::default().write(out, version),
        }
    }
}

/// 欄を宣言の順に読み書きする構造体。`欄: 型 = 件数` は決まった件数の表（Vec）
macro_rules! record {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$field_meta:meta])* pub $field:ident : $ty:ty $(= $count:expr)? , )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default)]
        pub struct $name {
            $( $(#[$field_meta])* pub $field: $ty, )*
        }

        impl Field for $name {
            fn read(reader: &mut Reader<'_>, version: u32) -> Result<Self> {
                Ok(Self { $( $field: record!(@read reader, version, $ty $(, $count)?), )* })
            }
            fn write(&self, out: &mut Writer, version: u32) {
                $( record!(@write out, version, self.$field $(, $count)?); )*
            }
        }
    };
    (@read $reader:ident, $version:ident, $ty:ty) => { <$ty as Field>::read($reader, $version)? };
    (@read $reader:ident, $version:ident, $ty:ty, $count:expr) => { read_table($reader, $version, $count)? };
    (@write $out:ident, $version:ident, $value:expr) => { Field::write(&$value, $out, $version) };
    (@write $out:ident, $version:ident, $value:expr, $count:expr) => { write_table($out, $version, &$value, $count) };
}

record! {
    /// レイヤの状態
    pub struct JwwLayerState {
        /// 0: 非表示、1: 表示のみ、2: 編集可能、3: 書込み
        pub state: u32,
        /// プロテクト（0: なし、1: 表示状態を変えられる、2: 表示状態も固定）
        pub protect: u32,
    }
}

impl Copy for JwwLayerState {}

record! {
    /// レイヤグループの状態
    pub struct JwwGroupState {
        /// 0: 非表示、1: 表示のみ、2: 編集可能、3: 書込み
        pub state: u32,
        /// 書込みレイヤ
        pub write_layer: u32,
        /// 縮尺の分母（1/100 なら 100）
        pub scale: f64,
        /// プロテクト（0: なし、1: 表示状態を変えられる、2: 表示状態も固定）
        pub protect: u32,
        /// 16 のレイヤ
        pub layers: [JwwLayerState; 16],
    }
}

record! {
    /// 名前より前の設定（寸法・印刷・目盛）
    pub struct JwwGeneralSettings {
        /// ダミー
        pub dummy: [u32; 14],
        /// 寸法の設定（m_lnSunpou1〜5。環境設定で保存するときだけ値を持ち、ほかは 0）
        pub dimension: [u32; 5],
        /// ダミー
        pub dummy2: u32,
        /// 線描画の最大幅。線幅を 1/100 mm 単位とするときは負（−101、版 600 以降は −201〜−300・−401〜−500）
        pub max_draw_width: i32,
        /// プリンタ出力範囲の原点
        pub print_origin: [f64; 2],
        /// プリンタ出力倍率
        pub print_scale: f64,
        /// 一位: プリンタ 90° 回転出力、十位: プリンタ出力基準点の位置
        pub print_setting: u32,
        /// 目盛設定モード（一位: 目盛表示、十位: 実寸指定。負なら目盛読取り不可）
        pub grid_mode: i32,
        /// 目盛表示の最小間隔（ドット）
        pub grid_min_dots: f64,
        /// 目盛表示の間隔
        pub grid_spacing: [f64; 2],
        /// 目盛の基準点
        pub grid_origin: [f64; 2],
    }
}

record! {
    /// マークジャンプ
    pub struct JwwMarkJump {
        /// 倍率
        pub scale: f64,
        /// 基準点
        pub origin: [f64; 2],
        /// レイヤグループ
        pub group: u32,
    }
}

impl Copy for JwwMarkJump {}

record! {
    /// 線色の画面表示色（COLORREF、0x00BBGGRR）と線幅
    pub struct JwwPen {
        pub color: u32,
        pub width: u32,
    }
}

impl Copy for JwwPen {}

record! {
    /// 線色のプリンタ出力色（COLORREF）・線幅・実点の半径（mm）
    pub struct JwwPrinterPen {
        pub color: u32,
        /// 線幅（印刷のドット、線幅を 1/100 mm 単位とするときは 1/100 mm）
        pub width: u32,
        pub point_radius: f64,
    }
}

impl Copy for JwwPrinterPen {}

record! {
    /// 線種のパターン（下位 unit ビットが 1 周期）・1 ユニットのドット数・ピッチ・プリンタ出力ピッチ
    pub struct JwwLinePattern {
        pub pattern: u32,
        pub unit: u32,
        pub pitch: u32,
        pub printer_pitch: u32,
    }
}

impl Copy for JwwLinePattern {}

record! {
    /// ランダム線のパターン・画面表示の振幅とピッチ・プリンタ出力の振幅とピッチ
    pub struct JwwRandomLine {
        pub pattern: u32,
        pub width: u32,
        pub pitch: u32,
        pub printer_width: u32,
        pub printer_pitch: u32,
    }
}

impl Copy for JwwRandomLine {}

record! {
    /// 名前より後ろの設定（日影・天空図・画面・線色・線種・印刷・2.5D・ソリッドの色）
    pub struct JwwDisplaySettings {
        /// 日影計算の測定面高さ
        pub shadow_level: f64,
        /// 緯度
        pub latitude: f64,
        /// 9〜15 時の測定の指定
        pub shadow_hours: u32,
        /// 壁面日影の測定面高さ
        pub wall_shadow_level: f64,
        /// 天空図の測定面高さ
        pub sky_level: f64,
        /// 天空図の半径 × 2
        pub sky_diameter: f64,
        /// 2.5D の計算単位（0 以外は mm 単位）
        pub mm_3d: u32,
        /// 保存時の画面倍率と原点
        pub view_scale: f64,
        pub view_origin: [f64; 2],
        /// 範囲記憶の倍率と基準点
        pub range_scale: f64,
        pub range_origin: [f64; 2],
        /// マークジャンプ 1〜8
        pub jumps: [JwwMarkJump; 8],
        /// 文字の描画状態のダミー（dDm11〜13、lnDm1、dDm21〜22）
        pub text_dummy: [f64; 3],
        pub text_dummy_int: u32,
        pub text_dummy2: [f64; 2],
        /// 文字列範囲を背景色で描くときの範囲の増寸法
        pub text_background_margin: f64,
        /// 十位: 文字を最後に描く、一位: 輪郭・範囲を背景色で描くか（1〜3）
        pub text_background: u32,
        /// 複線間隔
        pub parallel_spacings: [f64; 10],
        /// 両側複線の留線出の寸法
        pub double_parallel_end: f64,
        /// 線色 0〜9（0 は背景）の画面表示色と線幅
        pub pens: [JwwPen; 10],
        /// 線色 0〜9 のプリンタ出力色・線幅・実点の半径
        pub printer_pens: [JwwPrinterPen; 10],
        /// 線種 2〜9
        pub line_types: [JwwLinePattern; 8],
        /// ランダム線 1〜5（線種 11〜15）
        pub random_lines: [JwwRandomLine; 5],
        /// 倍長線種 6〜9（線種 16〜19）
        pub long_line_types: [JwwLinePattern; 4],
        /// 実点を画面で指定半径で描く
        pub screen_points: u32,
        /// 実点をプリンタで指定半径で書く
        pub printer_points: u32,
        /// 十位: ソリッドの描く順、一位: BitMap・ソリッドを最初に描く
        pub bitmap_first: u32,
        /// 逆描画
        pub reverse_draw: u32,
        /// 逆サーチ
        pub reverse_search: u32,
        /// カラー印刷
        pub color_print: u32,
        /// レイヤ順の印刷
        pub layer_order_print: u32,
        /// 色番号順の印刷
        pub color_order_print: u32,
        /// レイヤグループ・レイヤごとのプリンタ連続出力
        pub continuous_print: u32,
        /// 表示のみレイヤのグレー出力
        pub gray_print: u32,
        /// 表示のみレイヤを印刷しない。版 600 以降は、印刷の線幅の基準の dpi（300 dpi は +10、600 dpi は +20）を足す
        pub print_display_only: u32,
        /// 作図時間
        pub drawing_time: u32,
        /// 2.5D の視点を設定済みか（一位: 透視図、十位: 鳥瞰図、百位: アイソメ図）
        pub eye_set: u32,
        /// 2.5D の透視図・鳥瞰図・アイソメ図の視点水平角 × 100
        pub eye_angles: [u32; 3],
        /// 透視図の視点高さ・視点離れ、鳥瞰図の視点高さ・視点離れ、アイソメ図の視点垂直角
        pub eye: [f64; 5],
        /// 線の長さ指定の最終値
        pub last_length: f64,
        /// 矩形の横・縦寸法の最終値
        pub last_box: [f64; 2],
        /// 円の半径指定の最終値
        pub last_radius: f64,
        /// ソリッドを任意色で書く
        pub solid_any_color: u32,
        /// ソリッドの任意色の既定値（COLORREF）
        pub solid_color: u32,
    }
}

record! {
    /// SXF 対応拡張線色のプリンタ出力（線色名・色・線幅・点半径）
    pub struct JwwSxfPrinterPen {
        pub name: String,
        pub color: u32,
        pub width: u32,
        pub point_radius: f64,
    }
}

record! {
    /// SXF 対応拡張線種のパラメータ（線種名・セグメント数・線分と空白の長さ（mm）の繰り返し）
    pub struct JwwSxfLineType {
        pub name: String,
        pub segments: u32,
        pub pitches: [f64; 10],
    }
}

record! {
    /// SXF 対応拡張線色（線色 100〜356）と拡張線種（線種 30〜62）。版 420 以降
    pub struct JwwSxfSettings {
        /// 画面表示色と線幅（257 件）
        pub pens: Vec<JwwPen> = 257,
        /// 線色名・プリンタ出力色・線幅・点半径（257 件）
        pub printer_pens: Vec<JwwSxfPrinterPen> = 257,
        /// 線種のパターン（33 件）
        pub line_types: Vec<JwwLinePattern> = 33,
        /// 線種名・セグメント数・ピッチ（33 件）
        pub user_line_types: Vec<JwwSxfLineType> = 33,
    }
}

record! {
    /// 文字種の幅・高さ・間隔（mm）・色番号
    pub struct JwwTextStyle {
        pub width: f64,
        pub height: f64,
        pub spacing: f64,
        pub color: u32,
    }
}

impl Copy for JwwTextStyle {}

record! {
    /// 文字の設定
    pub struct JwwTextSettings {
        /// 文字種 1〜10
        pub styles: [JwwTextStyle; 10],
        /// 書込み文字の幅・高さ・間隔・色番号
        pub current: JwwTextStyle,
        /// 書込み文字の文字種
        pub current_style: u32,
        /// 文字位置整理の行間
        pub arrange_spacing: f64,
        /// 文字位置整理の文字数
        pub arrange_count: f64,
        /// 文字基準点のずれ位置を使う
        pub offset_enabled: u32,
        /// 文字基準点の横方向のずれ（左・中・右）
        pub offsets_x: [f64; 3],
        /// 文字基準点の縦方向のずれ（下・中・上）
        pub offsets_y: [f64; 3],
    }
}

/// 版 300 以降のヘッダの設定。欄は仕様の並び。読めなかった欄から後ろは None で、そのバイト列は `JwwHeader::rest`
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwwSettings {
    pub general: JwwGeneralSettings,
    /// レイヤ名（グループ × 16 + レイヤ、256 件）
    pub layer_names: Vec<String>,
    /// レイヤグループ名（16 件）
    pub group_names: Vec<String>,
    /// 名前より後ろの設定
    pub display: Option<JwwDisplaySettings>,
    /// SXF 対応拡張線色・線種（版 420 以降）
    pub sxf: Option<JwwSxfSettings>,
    /// 文字の設定
    pub text: Option<JwwTextSettings>,
}

/// ヘッダ。図形の一覧の前のすべて
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwwHeader {
    /// ファイルメモ
    pub memo: String,
    /// 図面サイズ（0〜4: A0〜A4、8: 2A、9: 3A、10: 4A、11: 5A、12: 10m、13: 50m、14: 100m）
    pub paper_size: u32,
    /// 書込みレイヤグループ
    pub write_group: u32,
    /// 16 のレイヤグループ
    pub groups: Vec<JwwGroupState>,
    /// 版 300 以降の設定
    pub settings: Option<JwwSettings>,
    /// 仕様の並びで読めなかった、図形の一覧の前までのバイト列（そのまま書く）
    pub rest: Vec<u8>,
}

/// 名前の件数
const LAYER_NAMES: usize = 256;
const GROUP_NAMES: usize = 16;

impl JwwHeader {
    /// bytes の先頭からヘッダを読む。end は図形の一覧の始まり。仕様の並びで読めないところからは rest に残す。
    /// 識別子・版・メモ・用紙・レイヤグループが読めなければエラー
    pub(crate) fn read(bytes: &[u8], end: usize, version: u32) -> Result<Self> {
        let bounded = bytes.get(..end).ok_or_else(|| ReadError::new(crate::archive::END_OF_DATA))?;
        let mut reader = Reader::new(bounded, 12);
        let memo = reader.cstring()?;
        let paper_size = reader.u32()?;
        let write_group = reader.u32()?;
        let groups = read_table(&mut reader, version, 16)?;
        let mut header = JwwHeader { memo, paper_size, write_group, groups, settings: None, rest: Vec::new() };
        let mut stop = reader.pos();
        if version >= 300 {
            header.settings = read_settings(bounded, &mut stop, version);
        }
        header.rest = bounded[stop..].to_vec();
        Ok(header)
    }

    /// ヘッダを書く（識別子と版から）
    pub(crate) fn write(&self, out: &mut Writer, version: u32) {
        out.raw(b"JwwData.");
        out.u32(version);
        self.memo.write(out, version);
        out.u32(self.paper_size);
        out.u32(self.write_group);
        write_table(out, version, &self.groups, 16);
        if version >= 300
            && let Some(settings) = &self.settings
        {
            write_settings(settings, out, version);
        }
        out.raw(&self.rest);
    }
}

/// 版 300 以降の設定を読む。stop は読めたところの終わり（欄ごとに進める）
fn read_settings(bytes: &[u8], stop: &mut usize, version: u32) -> Option<JwwSettings> {
    let mut reader = Reader::new(bytes, *stop);
    let general = JwwGeneralSettings::read(&mut reader, version).ok()?;
    let layer_names = read_table(&mut reader, version, LAYER_NAMES).ok()?;
    let group_names = read_table(&mut reader, version, GROUP_NAMES).ok()?;
    *stop = reader.pos();
    let mut settings = JwwSettings { general, layer_names, group_names, display: None, sxf: None, text: None };
    let Ok(display) = JwwDisplaySettings::read(&mut reader, version) else {
        return Some(settings);
    };
    settings.display = Some(display);
    *stop = reader.pos();
    if version >= 420 {
        let Ok(sxf) = JwwSxfSettings::read(&mut reader, version) else {
            return Some(settings);
        };
        settings.sxf = Some(sxf);
        *stop = reader.pos();
    }
    if let Ok(text) = JwwTextSettings::read(&mut reader, version) {
        settings.text = Some(text);
        *stop = reader.pos();
    }
    Some(settings)
}

/// 版 300 以降の設定を書く。読めなかった欄（None）から後ろは書かない（rest にある）
fn write_settings(settings: &JwwSettings, out: &mut Writer, version: u32) {
    settings.general.write(out, version);
    write_table(out, version, &settings.layer_names, LAYER_NAMES);
    write_table(out, version, &settings.group_names, GROUP_NAMES);
    let Some(display) = &settings.display else {
        return;
    };
    display.write(out, version);
    if version >= 420 {
        let Some(sxf) = &settings.sxf else {
            return;
        };
        sxf.write(out, version);
    }
    if let Some(text) = &settings.text {
        text.write(out, version);
    }
}
