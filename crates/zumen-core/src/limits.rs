//! 読み取りの上限（バイト数・図形の数・時間）。受け取った図面を信用できないサーバーで、重すぎる図面を
//! 途中で止めて理由を返すために使う。
//!
//! 上限は `with_limits` の中で動く読み取りにだけ効く（同じスレッド）。読み取りは図形を加えるたびに `admit`、
//! 重い繰り返しのたびに `tick` を呼び、上限を超えたら繰り返しをやめる。途中までの結果は呼び出し側が捨てる。
//! `with_limits` の外では何もしないので、上限を指定しなければ結果は TypeScript の読み取りと変わらない。

use crate::js;
use crate::read::{DrawingReader, ReadError, ReadOptions, read_drawing};
use crate::types::Scene;
use std::cell::Cell;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// 読み取りの上限。None は上限なし
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadLimits {
    /// 図面のバイト数（圧縮した SXF の P2Z・SFZ は、展開した後のバイト数）
    pub max_bytes: Option<u64>,
    /// 表示用の図面の図形の数
    pub max_entities: Option<u64>,
    /// 読み取りにかける時間。WebAssembly では使えず、無視する
    pub max_duration: Option<Duration>,
}

/// 超えた上限の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Limit {
    Bytes,
    Entities,
    Time,
}

impl Limit {
    /// 英語の名前（bytes、entities、time）
    pub fn name(self) -> &'static str {
        match self {
            Limit::Bytes => "bytes",
            Limit::Entities => "entities",
            Limit::Time => "time",
        }
    }
}

/// 時刻を確かめる間隔（`tick` の回数）。Instant::now は数十 ns かかるので毎回は呼ばない
#[cfg(not(target_arch = "wasm32"))]
const CLOCK_EVERY: u32 = 256;

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static MAX_BYTES: Cell<Option<u64>> = const { Cell::new(None) };
    static MAX_ENTITIES: Cell<u64> = const { Cell::new(u64::MAX) };
    static ENTITIES: Cell<u64> = const { Cell::new(0) };
    static TICKS: Cell<u32> = const { Cell::new(0) };
    #[cfg(not(target_arch = "wasm32"))]
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    static EXCEEDED: Cell<Option<Limit>> = const { Cell::new(None) };
}

/// with_limits を抜けるとき（panic でも）、上限を外す
struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        ACTIVE.set(false);
        MAX_BYTES.set(None);
        MAX_ENTITIES.set(u64::MAX);
        ENTITIES.set(0);
        TICKS.set(0);
        #[cfg(not(target_arch = "wasm32"))]
        DEADLINE.set(None);
        EXCEEDED.set(None);
    }
}

/// limits の上限で f を動かす。上限を超えたら、その種類も返す（そのときの f の結果は途中までのもの）。
/// 入れ子にはしない（内側の呼び出しは外側の上限を外す）
pub fn with_limits<T>(limits: &ReadLimits, f: impl FnOnce() -> T) -> (T, Option<Limit>) {
    let reset = Reset;
    ACTIVE.set(true);
    MAX_BYTES.set(limits.max_bytes);
    MAX_ENTITIES.set(limits.max_entities.unwrap_or(u64::MAX));
    ENTITIES.set(0);
    TICKS.set(0);
    #[cfg(not(target_arch = "wasm32"))]
    DEADLINE.set(limits.max_duration.and_then(|duration| Instant::now().checked_add(duration)));
    EXCEEDED.set(None);
    let value = f();
    let exceeded = EXCEEDED.get();
    drop(reset);
    (value, exceeded)
}

/// 展開した後のバイト数の上限（with_limits の中だけ）
pub fn max_bytes() -> Option<u64> {
    if ACTIVE.get() { MAX_BYTES.get() } else { None }
}

/// 上限を超えたことにする（展開の途中で大きさの上限を超えたときなど）
pub fn exceed(limit: Limit) {
    if ACTIVE.get() && EXCEEDED.get().is_none() {
        EXCEEDED.set(Some(limit));
    }
}

/// 図形を 1 つ加えてよいか。図形の数か時間の上限を超えたら false（以後も false）
pub fn admit() -> bool {
    if !ACTIVE.get() {
        return true;
    }
    if EXCEEDED.get().is_some() {
        return false;
    }
    let count = ENTITIES.get() + 1;
    ENTITIES.set(count);
    if count > MAX_ENTITIES.get() {
        EXCEEDED.set(Some(Limit::Entities));
        return false;
    }
    !tick()
}

/// 加えた図形を count 個取り除いた（1 つの図形にまとめたときなど）。数え直して、上限の判断を最後の図形の数に合わせる
pub fn release(count: u64) {
    if ACTIVE.get() {
        ENTITIES.set(ENTITIES.get().saturating_sub(count));
    }
}

/// 重い繰り返しの中で呼ぶ。時間の上限を超えたか、ほかの上限ですでに止めていれば true（繰り返しをやめる）
pub fn tick() -> bool {
    if !ACTIVE.get() {
        return false;
    }
    if EXCEEDED.get().is_some() {
        return true;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(deadline) = DEADLINE.get() {
        let ticks = TICKS.get();
        TICKS.set(ticks.wrapping_add(1));
        if ticks.is_multiple_of(CLOCK_EVERY) && Instant::now() >= deadline {
            EXCEEDED.set(Some(Limit::Time));
            return true;
        }
    }
    false
}

/// 上限を超えて止めているか
pub fn stopped() -> bool {
    ACTIVE.get() && EXCEEDED.get().is_some()
}

/// 上限を超えたときのエラー（利用者に見せる文）
pub fn limit_error(limit: Limit, limits: &ReadLimits) -> ReadError {
    let count = |value: Option<u64>| value.map_or_else(String::new, |value| js::format_ja(value as f64, 0));
    let message = match limit {
        Limit::Bytes => format!("図面が上限の {} バイトより大きいため、読みませんでした", count(limits.max_bytes)),
        Limit::Entities => {
            format!("図形が上限の {} 個を超えたため、読むのを途中でやめました", count(limits.max_entities))
        }
        Limit::Time => {
            let seconds = limits.max_duration.map_or(0.0, |duration| duration.as_secs_f64());
            format!("読み取りが上限の {} 秒を超えたため、途中でやめました", js::number_to_string(seconds))
        }
    };
    ReadError::limit(limit, message)
}

/// 渡した読み取りで図面を読む（`read_drawing`）。limits の上限を超えたら、途中でやめて上限のエラーを返す。
/// zumen::read と、形式ごとの WebAssembly のモジュールが使う
pub fn read_drawing_with_limits(
    bytes: &[u8],
    readers: &[&dyn DrawingReader],
    format: Option<&str>,
    options: &ReadOptions,
    limits: &ReadLimits,
) -> Result<Scene, ReadError> {
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(limit_error(Limit::Bytes, limits));
    }
    let (result, exceeded) = with_limits(limits, || read_drawing(bytes, readers, format, options));
    // 上限を超えたときは、途中までの結果や、途中でやめたために起きた読み取りのエラーより上限のエラーを返す
    match (exceeded, result) {
        (Some(_), Err(error)) if error.exceeded().is_some() => Err(error),
        (Some(limit), _) => Err(limit_error(limit, limits)),
        (None, result) => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_entities_and_stops_at_the_limit() {
        let limits = ReadLimits { max_entities: Some(3), ..ReadLimits::default() };
        let (admitted, exceeded) = with_limits(&limits, || (0..10).filter(|_| admit()).count());
        assert_eq!((admitted, exceeded), (3, Some(Limit::Entities)));
        // with_limits の外では何もしない
        assert!(admit() && !tick() && !stopped());
        assert_eq!(max_bytes(), None);
    }

    #[test]
    fn stops_when_the_time_is_up() {
        let limits = ReadLimits { max_duration: Some(Duration::ZERO), ..ReadLimits::default() };
        let ((), exceeded) = with_limits(&limits, || {
            assert!(tick());
            assert!(stopped() && !admit());
        });
        assert_eq!(exceeded, Some(Limit::Time));
        let ((), none) = with_limits(&ReadLimits::default(), || assert!(!tick() && admit()));
        assert_eq!(none, None);
    }

    #[test]
    fn reports_the_limit_set_by_a_reader() {
        let limits = ReadLimits { max_bytes: Some(10), ..ReadLimits::default() };
        let (bytes, exceeded) = with_limits(&limits, || {
            let bytes = max_bytes();
            exceed(Limit::Bytes);
            bytes
        });
        assert_eq!((bytes, exceeded), (Some(10), Some(Limit::Bytes)));
    }
}
