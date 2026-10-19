//! ファイルのヘッダ（FILE_DESCRIPTION・FILE_NAME）の文字列を取り出す。TypeScript の正規表現
//! `ENTITY\s*\(([\s\S]*?)\)\s*;` と `'((?:[^']|'')*)'` と同じ所に一致させる（`\s` は JavaScript の空白）。

use zumen_core::js;

/// ファイルのヘッダ（FILE_DESCRIPTION・FILE_NAME）の文字列
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SxfHeader {
    pub description: String,
    pub file_name: String,
    pub preprocessor: String,
    pub originating_system: String,
}

fn skip_space(text: &str, from: usize) -> usize {
    text[from..].find(|c: char| !js::is_whitespace(c)).map_or(text.len(), |offset| from + offset)
}

/// `ENTITY\s*\(` の後ろから、`\)\s*;` が続く最初の `)` までの中身
fn entity_args<'a>(text: &'a str, entity: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(found) = text[from..].find(entity) {
        let at = from + found;
        let open = skip_space(text, at + entity.len());
        if text[open..].starts_with('(') {
            let start = open + 1;
            let mut search = start;
            while let Some(offset) = text[search..].find(')') {
                let close = search + offset;
                if text[skip_space(text, close + 1)..].starts_with(';') {
                    return Some(&text[start..close]);
                }
                search = close + 1;
            }
        }
        // 名前は ASCII なので、1 バイト先は文字の境目
        from = at + 1;
    }
    None
}

/// `'…'` の中身を並び順に返す。`''` は引用符 1 つとして読み飛ばす。閉じる引用符がなく終わるときは、
/// 正規表現の後戻りと同じく、最後の `''` の 1 つ目で閉じたとみなす
fn quoted(args: &str) -> Vec<&str> {
    let bytes = args.as_bytes();
    let mut out = Vec::new();
    let mut pos = 0;
    while let Some(offset) = bytes[pos..].iter().position(|byte| *byte == b'\'') {
        let open = pos + offset;
        let mut i = open + 1;
        let mut last_pair = None;
        while i < bytes.len() {
            if bytes[i] != b'\'' {
                i += 1;
            } else if bytes.get(i + 1) == Some(&b'\'') {
                last_pair = Some(i);
                i += 2;
            } else {
                break;
            }
        }
        if i < bytes.len() {
            out.push(&args[open + 1..i]);
            pos = i + 1;
        } else if let Some(pair) = last_pair {
            out.push(&args[open + 1..pair]);
            pos = pair + 1;
        } else {
            pos = open + 1;
        }
    }
    out
}

/// 実体の引数の文字列。中身が空なら空の一覧（TypeScript の `!match?.[1]`）
pub fn header_strings<'a>(text: &'a str, entity: &str) -> Vec<&'a str> {
    match entity_args(text, entity) {
        Some(args) if !args.is_empty() => quoted(args),
        _ => Vec::new(),
    }
}

/// FILE_DESCRIPTION の 1 つ目と、FILE_NAME の 1・5・6 つ目。TypeScript と同じく、使わない文字列も
/// 並び順に decode で戻す（戻せない文字列があれば、そこで失敗する）
pub fn read_header<E>(text: &str, mut decode: impl FnMut(&str) -> Result<String, E>) -> Result<SxfHeader, E> {
    let mut strings = |entity: &str| -> Result<Vec<String>, E> {
        header_strings(text, entity).into_iter().map(&mut decode).collect()
    };
    let description = strings("FILE_DESCRIPTION")?;
    let name = strings("FILE_NAME")?;
    let get = |list: &[String], index: usize| list.get(index).cloned().unwrap_or_default();
    Ok(SxfHeader {
        description: get(&description, 0),
        file_name: get(&name, 0),
        preprocessor: get(&name, 4),
        originating_system: get(&name, 5),
    })
}
