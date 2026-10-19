use crate::limits::Limit;
use crate::types::Scene;
use std::fmt;

/// 読み取りの失敗。メッセージは利用者に見せる文（TypeScript の読み取りと同じ文）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    message: String,
    limit: Option<Limit>,
}

impl ReadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), limit: None }
    }

    /// 読み取りの上限（`ReadLimits`）を超えたための失敗
    pub fn limit(limit: Limit, message: impl Into<String>) -> Self {
        Self { message: message.into(), limit: Some(limit) }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// 上限を超えたための失敗なら、その上限
    pub fn exceeded(&self) -> Option<Limit> {
        self.limit
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReadError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadOptions {
    /// 描く表示（Scene.views の id）。DXF・DWG のレイアウトなど。省略すると形式ごとの既定の表示
    pub view: Option<String>,
}

/// 形式ごとの読み取り。形式は拡張子ではなく中身で判定する
pub trait DrawingReader: Send + Sync {
    /// 読める形式の名前。Scene.format と同じ値
    fn formats(&self) -> &'static [&'static str];
    /// 中身がこの読み取りの形式なら、その名前を返す
    fn detect(&self, bytes: &[u8]) -> Option<&'static str>;
    /// 表示用の図面にする。format は detect の結果か、利用者が指定した形式
    fn read(&self, bytes: &[u8], format: &str, options: &ReadOptions) -> Result<Scene, ReadError>;
}

/// 渡した読み取りのうち、最初に中身を認めたものの形式を返す
pub fn detect_format(bytes: &[u8], readers: &[&dyn DrawingReader]) -> Option<&'static str> {
    readers.iter().find_map(|reader| reader.detect(bytes))
}

/// 図面のバイト列を、渡した読み取りのどれかで読み、表示用の図面にする。format は分かっている場合の形式
pub fn read_drawing(
    bytes: &[u8],
    readers: &[&dyn DrawingReader],
    format: Option<&str>,
    options: &ReadOptions,
) -> Result<Scene, ReadError> {
    if let Some(format) = format {
        let reader = readers
            .iter()
            .find(|reader| reader.formats().contains(&format))
            .ok_or_else(|| ReadError::new(format!("{} を読む読み取りが渡されていません", format.to_uppercase())))?;
        return reader.read(bytes, format, options);
    }
    for reader in readers {
        if let Some(format) = reader.detect(bytes) {
            return reader.read(bytes, format, options);
        }
    }
    let known: Vec<String> =
        readers.iter().flat_map(|reader| reader.formats().iter().map(|format| format.to_uppercase())).collect();
    let list = if known.is_empty() { "なし".to_string() } else { known.join("・") };
    Err(ReadError::new(format!("対応していない形式のファイルです（読める形式: {list}）")))
}
