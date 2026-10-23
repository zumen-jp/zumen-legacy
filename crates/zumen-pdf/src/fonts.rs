//! 字形の並びのフォントの名前と、ベースラインに対する em の位置
//!
//! hayro-interpret は字形にフォントの名前を付けないが、フォントを見分ける値（`font_cache_key`）はフォントの辞書の
//! バイト列から作る（`Dict::cache_key`）。ページの資源（と、その中のフォーム XObject と Type 3 フォントの資源）の
//! フォントの辞書から同じ値を作り、名前（BaseFont）と FontDescriptor の Ascent・Descent を引けるようにする。

use crate::strings;
use hayro_interpret::CacheKey;
use hayro_interpret::hayro_syntax::object::{Array, Dict, Name, Stream};
use hayro_interpret::hayro_syntax::page::Resources;
use std::collections::{HashMap, HashSet};
use zumen_core::limits;

/// 資源をたどる深さと、たどるフォーム XObject の数の上限
const MAX_DEPTH: usize = 16;
const MAX_FORMS: usize = 4096;

/// FontDescriptor がないフォント（標準の 14 フォントなど）の Ascent と Descent（字形の単位、em の 1/1000）
const DEFAULT_ASCENT: f64 = 800.0;
const DEFAULT_DESCENT: f64 = -200.0;

/// フォントの名前と、em の上端・下端（ベースラインからの高さ。em の 1/1000）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FontInfo {
    pub name: String,
    pub ascent: f64,
    pub descent: f64,
}

impl FontInfo {
    fn unknown() -> Self {
        Self { name: String::new(), ascent: DEFAULT_ASCENT, descent: DEFAULT_DESCENT }
    }
}

/// フォントを見分ける値 → フォントの情報。最初に要るときにページの資源から作る
pub(crate) struct Fonts<'a> {
    resources: Resources<'a>,
    table: Option<HashMap<u128, FontInfo>>,
    unknown: FontInfo,
}

impl<'a> Fonts<'a> {
    pub(crate) fn new(resources: &Resources<'a>) -> Self {
        Self { resources: resources.clone(), table: None, unknown: FontInfo::unknown() }
    }

    /// フォントの情報。分からないフォント（Type 3、読めずに標準のフォントで代えたもの）は名前が空
    pub(crate) fn get(&mut self, key: Option<u128>) -> &FontInfo {
        let resources = &self.resources;
        let table = self.table.get_or_insert_with(|| {
            let mut table = HashMap::new();
            let mut forms = HashSet::new();
            collect(&resources.fonts, &resources.x_objects, 0, &mut forms, &mut table);
            table
        });
        key.and_then(|key| table.get(&key)).unwrap_or(&self.unknown)
    }
}

fn collect(
    fonts: &Dict<'_>,
    x_objects: &Dict<'_>,
    depth: usize,
    forms: &mut HashSet<Vec<u8>>,
    table: &mut HashMap<u128, FontInfo>,
) {
    if depth > MAX_DEPTH || limits::tick() {
        return;
    }
    for key in fonts.keys() {
        let Some(font) = fonts.get::<Dict<'_>>(key.as_ref()) else { continue };
        table.entry(font.cache_key()).or_insert_with(|| info(&font));
        // Type 3 フォントの字形は資源を持てる（9.6.5）
        if let Some(resources) = font.get::<Dict<'_>>(b"Resources") {
            nested(&resources, depth, forms, table);
        }
    }
    for key in x_objects.keys() {
        let Some(stream) = x_objects.get::<Stream<'_>>(key.as_ref()) else { continue };
        let dict = stream.dict();
        if dict.get::<Name<'_>>(b"Subtype").as_deref() != Some(b"Form".as_slice()) {
            continue;
        }
        if forms.len() >= MAX_FORMS || !forms.insert(dict.data().to_vec()) {
            continue;
        }
        if let Some(resources) = dict.get::<Dict<'_>>(b"Resources") {
            nested(&resources, depth, forms, table);
        }
    }
}

fn nested(resources: &Dict<'_>, depth: usize, forms: &mut HashSet<Vec<u8>>, table: &mut HashMap<u128, FontInfo>) {
    let fonts = resources.get::<Dict<'_>>(b"Font").unwrap_or_default();
    let x_objects = resources.get::<Dict<'_>>(b"XObject").unwrap_or_default();
    collect(&fonts, &x_objects, depth + 1, forms, table);
}

/// フォントの辞書（9.6、9.7）の名前と em の上端・下端。Type 0 の FontDescriptor は子孫の CID フォントにある
fn info(font: &Dict<'_>) -> FontInfo {
    let name =
        font.get::<Name<'_>>(b"BaseFont").map(|name| without_subset(&strings::name(name.as_ref()))).unwrap_or_default();
    let descriptor = font.get::<Dict<'_>>(b"FontDescriptor").or_else(|| {
        font.get::<Array<'_>>(b"DescendantFonts")
            .and_then(|fonts| fonts.iter::<Dict<'_>>().next())
            .and_then(|descendant| descendant.get::<Dict<'_>>(b"FontDescriptor"))
    });
    let ascent = descriptor.as_ref().and_then(|descriptor| descriptor.get::<f64>(b"Ascent"));
    let descent = descriptor.as_ref().and_then(|descriptor| descriptor.get::<f64>(b"Descent"));
    match (ascent, descent) {
        // 上端が下端より上にあり、em の 2 倍を超えない値だけを使う
        (Some(ascent), Some(descent))
            if ascent.is_finite() && descent.is_finite() && ascent > descent && ascent - descent <= 2000.0 =>
        {
            FontInfo { name, ascent, descent }
        }
        _ => FontInfo { name, ..FontInfo::unknown() },
    }
}

/// 埋め込んだ部分集合の名前の前置き（大文字 6 文字と +、9.6.4）を除く
fn without_subset(name: &str) -> String {
    match name.split_once('+') {
        Some((tag, rest)) if tag.len() == 6 && tag.bytes().all(|byte| byte.is_ascii_uppercase()) => rest.to_string(),
        _ => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_subset_tag() {
        assert_eq!(without_subset("ABCDEF+BIZUDGothic"), "BIZUDGothic");
        assert_eq!(without_subset("Arial+Bold"), "Arial+Bold");
        assert_eq!(without_subset("Helvetica"), "Helvetica");
    }
}
