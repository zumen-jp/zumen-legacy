//! 元の P21 を書き換えて、図面（`SxfDocument`）と違うフィーチャのインスタンスだけを書き直す。変えていない
//! インスタンスは元の文字列のまま残す（その場の編集）。
//!
//! 図面の要素がどのインスタンスから来たかは読み取り（p21.rs の P21Roots）が覚えている。用紙・複合図形の要素の並び
//! （DRAWING_SHEET_REVISION・DRAUGHTING_SUBFIGURE_REPRESENTATION の items）と複合曲線ごとに、要素をインスタンス番号で
//! 図面と対応させる。
//!
//! - 値が変わった要素は、要素の並びに入っているインスタンス（occurrence・寸法の callout）の番号を変えずに書き直し、
//!   形・見え方のインスタンスは新しい番号で作る。元の要素だけを参照する関係（DIMENSION_CALLOUT_RELATIONSHIP）は消す。
//! - 図面にない要素は要素の並びから外し、新しい要素は新しい番号で作って要素の並びに入れる。
//! - その後、どこからも参照されていなかった元のインスタンス（と新しいインスタンス）を起点に参照を辿り、辿れなくなった
//!   インスタンスを消す。レイヤの割り当てと INVISIBILITY の参照は辿らず（弱い参照）、消したインスタンスを外すだけにする。
//!   要素がなくなったレイヤの割り当ては消す（assigned_items は 1 つ以上）。
//! - レイヤの名前・表示、用紙の大きさ・名前、色・線種・線幅・文字フォントの値が変わったら、そのインスタンスを書き直す。
//!   新しいレイヤ・色などは、元より大きい番号で作る（読み取りは番号の順にコードを割り当てる）。
//! - 新しいインスタンスは DATA 節の最後に、元の最大の番号より大きい 10 の倍数から振る。改行は元のファイルに合わせる。
//! - 番号の重なるインスタンスがある・用紙がない P21 は、どのインスタンスを書き換えればよいか決められないので、新しく書く。

use crate::document::{CompositeCurve, SubFigure, SxfDocument, SxfDrawing};
use crate::features::{DrawingSheet, SxfFeature};
use crate::p21::{P21Roots, convert_p21};
use crate::p21_reader::{P21Entity, P21File, P21Value};
use crate::p21_writer::{DUMMY_LAYER, Graph, write_p21_new};
use crate::splice::{Splices, line_ending, skip_space, with_line_ending};
use crate::step::{Body, StepWriter, V, part};
use crate::tables::sheet_size;
use crate::write::{SxfWritten, sub_figure_order};
use std::collections::{HashMap, HashSet, VecDeque};
use zumen_core::{Notice, NoticeLevel, NoticeList};

/// インスタンス番号（読み取りは f64 で持つ）を整数にする
fn id(value: f64) -> u64 {
    value as u64
}

/// 値の中の参照
fn references(value: &P21Value, out: &mut Vec<u64>) {
    match value {
        P21Value::Ref(reference) => out.push(id(*reference)),
        P21Value::List(items) => items.iter().for_each(|item| references(item, out)),
        P21Value::Typed(_, inner) => references(inner, out),
        _ => {}
    }
}

fn value_references(value: &V, out: &mut Vec<u64>) {
    match value {
        V::Ref(reference) => out.push(*reference),
        V::List(items) => items.iter().for_each(|item| value_references(item, out)),
        V::Typed(_, inner) => value_references(inner, out),
        _ => {}
    }
}

/// 参照を辿る起点にしない（弱い）参照の引数。レイヤの割り当ての要素と、非表示の要素
fn weak_argument(name: &str) -> Option<usize> {
    match name {
        "PRESENTATION_LAYER_ASSIGNMENT" => Some(2),
        "INVISIBILITY" => Some(0),
        _ => None,
    }
}

/// 要素の並び（items）の引数。要素を消すと、並びから外す
fn container_argument(name: &str) -> Option<usize> {
    match name {
        "DRAWING_SHEET_REVISION" | "DRAUGHTING_SUBFIGURE_REPRESENTATION" => Some(1),
        _ => None,
    }
}

/// 読んだインスタンスの強い参照（弱い参照の引数を除く）
fn strong_references(parts: &[(String, Vec<P21Value>)]) -> Vec<u64> {
    let mut out = Vec::new();
    for (name, args) in parts {
        for (position, value) in args.iter().enumerate() {
            if weak_argument(name) != Some(position) {
                references(value, &mut out);
            }
        }
    }
    out
}

/// 作ったインスタンスの強い参照
fn body_references(body: &Body) -> Vec<u64> {
    let parts = match body {
        Body::Simple(part) => std::slice::from_ref(part),
        Body::Complex(parts) => parts.as_slice(),
    };
    let mut out = Vec::new();
    for part in parts {
        for (position, value) in part.args.iter().enumerate() {
            if weak_argument(part.name) != Some(position) {
                value_references(value, &mut out);
            }
        }
    }
    out
}

/// 元の要素の並びの 1 つ（要素の並びのインスタンスと、それから読んだフィーチャ）
struct SourceMember<'d> {
    item: u64,
    feature: &'d SxfFeature,
}

/// 要素の並びとフィーチャを組にする（フィーチャにならない要素は飛ばす）
fn source_members<'d>(items: &[(f64, usize)], features: &'d [SxfFeature]) -> Vec<SourceMember<'d>> {
    let mut out = Vec::new();
    let mut features = features.iter();
    for (item, count) in items {
        for _ in 0..*count {
            if let Some(feature) = features.next() {
                out.push(SourceMember { item: id(*item), feature });
            }
        }
    }
    out
}

/// フィーチャのインスタンス番号で前から対応させる。並びの順が入れ替わった要素は対応させない。
/// 図面の要素ごとの対応する元の要素（添字）と、対応しない元の要素（添字）を返す
fn pair(source: &[SourceMember<'_>], current: &[SxfFeature]) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut by_id: HashMap<u64, VecDeque<usize>> = HashMap::new();
    for (position, member) in source.iter().enumerate() {
        by_id.entry(member.feature.id().to_bits()).or_default().push_back(position);
    }
    let mut taken = vec![false; source.len()];
    let mut last: Option<usize> = None;
    let matched = current
        .iter()
        .map(|feature| {
            let position = by_id.get_mut(&feature.id().to_bits())?.pop_front()?;
            if last.is_some_and(|last| position < last) {
                return None;
            }
            taken[position] = true;
            last = Some(position);
            Some(position)
        })
        .collect();
    let unmatched = (0..source.len()).filter(|position| !taken[*position]).collect();
    (matched, unmatched)
}

/// 要素の並びの書き換え
#[derive(Default)]
struct ListEdit {
    /// 外す要素
    removed: HashSet<u64>,
    /// 入れる要素と、その直前の元の要素（None は並びの最初）
    inserted: Vec<(Option<u64>, u64)>,
}

/// 書き直す元のインスタンスの新しい本体
enum Rewrite {
    /// 書き出しで作った本体
    Body(Body),
    /// 読んだ本体を変えたもの
    Parts(Vec<(String, Vec<P21Value>)>),
}

struct Editor<'a, 'd> {
    file: &'a P21File,
    doc: &'d SxfDocument,
    original: &'a SxfDocument,
    roots: &'a P21Roots,
    /// 元のインスタンスを強く参照するインスタンス
    referrers: HashMap<u64, Vec<u64>>,
    graph: Graph<'d>,
    rewritten: HashMap<u64, Rewrite>,
    /// 消す元のインスタンス
    dead: HashSet<u64>,
    /// 要素の並びの書き換え（DRAWING_SHEET_REVISION・DRAUGHTING_SUBFIGURE_REPRESENTATION → 書き換え）
    lists: HashMap<u64, ListEdit>,
    /// レイヤの割り当てから外す要素（値を変えた要素は、新しいレイヤに入れ直す）
    unassigned: HashSet<u64>,
    /// 非表示にする・表示に戻す元のインスタンス（レイヤの割り当て・複合曲線の occurrence）
    hide: Vec<u64>,
    show: HashSet<u64>,
    notices: NoticeList,
    ids: Vec<(f64, f64)>,
}

impl<'a, 'd> Editor<'a, 'd> {
    fn entity(&self, instance: u64) -> Option<&'a P21Entity> {
        self.file.entities.get(instance as f64)
    }

    /// 書き出しで start 以降に作ったインスタンスのうち new を、元の番号 id のインスタンスとして書き直す。
    /// 作ったインスタンスの中の new への参照は id への参照にする
    fn adopt(&mut self, start: usize, new: u64, id: u64) {
        fn rename(value: &mut V, new: u64, id: u64) {
            match value {
                V::Ref(reference) if *reference == new => *reference = id,
                V::List(items) => items.iter_mut().for_each(|item| rename(item, new, id)),
                V::Typed(_, inner) => rename(inner, new, id),
                _ => {}
            }
        }
        let mut adopted = None;
        let mut index = start;
        while index < self.graph.instances.len() {
            let (instance, body) = &mut self.graph.instances[index];
            let parts = match body {
                Body::Simple(part) => std::slice::from_mut(part),
                Body::Complex(parts) => parts.as_mut_slice(),
            };
            for part in parts {
                part.args.iter_mut().for_each(|value| rename(value, new, id));
            }
            if *instance == new {
                adopted = Some(self.graph.instances.remove(index).1);
            } else {
                index += 1;
            }
        }
        for items in self.graph.layer_items.values_mut() {
            items.iter_mut().filter(|item| **item == new).for_each(|item| *item = id);
        }
        self.graph.invisible.iter_mut().filter(|item| **item == new).for_each(|item| *item = id);
        for (_, written) in self.graph.ids.iter_mut().filter(|(_, written)| *written == new as f64) {
            *written = id as f64;
        }
        if let Some(body) = adopted {
            self.rewritten.insert(id, Rewrite::Body(body));
        }
    }

    /// 元のインスタンスを書き直さず、読んだ本体を変えて書く
    fn rewrite_parts(&mut self, instance: u64, change: impl FnOnce(&mut Vec<(String, Vec<P21Value>)>)) {
        let Some(entity) = self.entity(instance) else {
            return;
        };
        let mut parts = match self.rewritten.remove(&instance) {
            Some(Rewrite::Parts(parts)) => parts,
            _ => entity.parts.clone(),
        };
        change(&mut parts);
        self.rewritten.insert(instance, Rewrite::Parts(parts));
    }

    /// 要素を新しく作り、要素の並びに入れるインスタンスを返す（書いた番号は graph.ids に入る）
    fn add_member(&mut self, feature: &SxfFeature) -> Option<u64> {
        self.graph.member(feature)
    }

    /// 値が変わった要素を、要素の並びのインスタンスの番号を変えずに書き直す。書き直せなければ false
    fn rewrite_member(&mut self, item: u64, feature: &SxfFeature) -> bool {
        if item as f64 != feature.id() || self.entity(item).is_none() {
            return false;
        }
        let start = self.graph.instances.len();
        let Some(new) = self.graph.member(feature) else {
            return false;
        };
        self.adopt(start, new, item);
        // 元の要素を参照していた関係（寸法の DIMENSION_CALLOUT_RELATIONSHIP など）は作り直したので消す。
        // 要素の並び・レイヤの割り当て・非表示の指定は残す
        for referrer in self.referrers.get(&item).cloned().unwrap_or_default() {
            let keep = self.entity(referrer).is_some_and(|entity| {
                entity.parts.iter().any(|(name, _)| container_argument(name).is_some() || weak_argument(name).is_some())
            });
            if !keep {
                self.dead.insert(referrer);
            }
        }
        self.unassigned.insert(item);
        true
    }

    /// 要素の並び（用紙・複合図形）を、図面の要素と対応させて書き換える
    fn members(&mut self, list: u64, source: &[SourceMember<'_>], current: &[SxfFeature]) {
        let (matched, unmatched) = pair(source, current);
        let mut edit = ListEdit::default();
        for position in unmatched {
            edit.removed.insert(source[position].item);
            self.dead.insert(source[position].item);
        }
        let mut previous: Option<u64> = None;
        for (position, feature) in current.iter().enumerate() {
            if let Some(index) = matched[position] {
                let member = &source[index];
                previous = Some(member.item);
                if member.feature == feature {
                    self.ids.push((feature.id(), feature.id()));
                    continue;
                }
                if self.rewrite_member(member.item, feature) {
                    continue;
                }
                // 書き直せない要素（ほかのインスタンスに包まれた寸法など）は、消して新しく作る
                edit.removed.insert(member.item);
                self.dead.insert(member.item);
            }
            if let Some(root) = self.add_member(feature) {
                edit.inserted.push((previous, root));
            }
        }
        self.lists.insert(list, edit);
    }
}

/// 元の P21 を書き換えられないとき、新しく書く
fn rewrite_all(drawing: &SxfDrawing, reason: &str) -> SxfWritten {
    let mut written = write_p21_new(drawing);
    written.notices.insert(
        0,
        Notice {
            level: NoticeLevel::Warning,
            code: "p21-rewrite".into(),
            message: format!("元の P21 を書き換えられないため、新しく書きました（{reason}）"),
            count: 1,
        },
    );
    written
}

/// DATA 節で使われている最大のインスタンス番号（読めないインスタンスの中も含む）
fn max_reference(data: &[u8]) -> u64 {
    let mut max = 0u64;
    let mut index = 0;
    while index < data.len() {
        if data[index] != b'#' {
            index += 1;
            continue;
        }
        let digits = data[index + 1..].iter().take_while(|byte| byte.is_ascii_digit()).count();
        if (1..19).contains(&digits) {
            let number =
                std::str::from_utf8(&data[index + 1..index + 1 + digits]).ok().and_then(|text| text.parse().ok());
            max = max.max(number.unwrap_or(0));
        }
        index += digits + 1;
    }
    max
}

/// 元の P21（bytes）を書き換えて、図面を書く
pub(crate) fn write_p21_edit(drawing: &SxfDrawing, bytes: &[u8]) -> SxfWritten {
    let Ok((original, file, roots)) = convert_p21(bytes) else {
        return rewrite_all(drawing, "元のファイルを読めない");
    };
    // 何も変えていなければ、元のファイルのまま
    if original.document == drawing.document {
        return SxfWritten { bytes: bytes.to_vec(), notices: Vec::new(), ids: Vec::new() };
    }
    let mut seen = HashSet::new();
    if file.spans.iter().filter_map(|span| span.id).any(|instance| !seen.insert(id(instance))) {
        return rewrite_all(drawing, "番号の重なるインスタンスがある");
    }
    let Some(sheet) = roots.sheet.map(id) else {
        return rewrite_all(drawing, "用紙（DRAWING_SHEET_REVISION）がない");
    };
    let doc = &drawing.document;
    let data = bytes.windows(5).position(|window| window == b"DATA;").unwrap_or(0);
    let first = (max_reference(&bytes[data..file.data_end.min(bytes.len())]) / 10 + 1) * 10;
    let mut referrers: HashMap<u64, Vec<u64>> = HashMap::new();
    for entity in file.entities.values() {
        for target in strong_references(&entity.parts) {
            referrers.entry(target).or_default().push(id(entity.id));
        }
    }
    let mut editor = Editor {
        file: &file,
        doc,
        original: &original.document,
        roots: &roots,
        referrers,
        graph: Graph::new(doc, first, 10),
        rewritten: HashMap::new(),
        dead: HashSet::new(),
        lists: HashMap::new(),
        unassigned: HashSet::new(),
        hide: Vec::new(),
        show: HashSet::new(),
        notices: NoticeList::new(),
        ids: Vec::new(),
    };
    prefill(&mut editor);
    tables(&mut editor);
    composite_curves(&mut editor);
    sub_figures(&mut editor);
    let source = source_members(&roots.sheet_items, &original.document.sheet_members);
    editor.members(sheet, &source, &doc.sheet_members);
    if let (Some(before), Some(after)) = (&original.document.sheet, &doc.sheet)
        && before != after
    {
        resize_sheet(&mut editor, sheet, after);
    }
    if doc.title.is_some() {
        editor.notices.add(
            NoticeLevel::Info,
            "title",
            "P21 の図面表題欄は読み取りが読まないため書き換えず、元のまま残しました",
        );
    }
    finish(editor, sheet, bytes)
}

/// 書き出しの表に、元のインスタンス（色・線種・線幅・文字フォント・単位・複合曲線・複合図形）を入れる。
/// 変えない表の項目は元のインスタンスを参照し、新しい項目だけを作る
fn prefill(editor: &mut Editor<'_, '_>) {
    let (doc, original, file, roots) = (editor.doc, editor.original, editor.file, editor.roots);
    // 既定義の色・線種は、同じコードの元のインスタンスのうち番号の小さいもの
    let mut predefined: Vec<(f64, f64)> =
        roots.colours.iter().map(|(instance, code)| (f64::from_bits(*instance), *code)).collect();
    predefined.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (instance, code) in predefined {
        if code <= 16.0 {
            editor.graph.colours.entry(code as i64).or_insert(id(instance));
        }
    }
    let mut predefined: Vec<(f64, f64)> =
        roots.line_types.iter().map(|(instance, code)| (f64::from_bits(*instance), *code)).collect();
    predefined.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (instance, code) in predefined {
        let pre_defined =
            file.entities.get(instance).is_some_and(|entity| entity.has("DRAUGHTING_PRE_DEFINED_CURVE_FONT"));
        if code <= 16.0 && pre_defined {
            editor.graph.line_types.entry(code as i64).or_insert(id(instance));
        }
    }
    // ユーザ定義の色・線種・文字フォントは、表の項目の番号が元のインスタンス
    for (index, colour) in doc.user_colors.iter().enumerate() {
        if original.user_colors.iter().any(|before| before.id == colour.id) {
            editor.graph.colours.insert(17 + index as i64, id(colour.id));
        }
    }
    for (index, line_type) in doc.user_line_types.iter().enumerate() {
        if original.user_line_types.iter().any(|before| before.id == line_type.id) {
            editor.graph.line_types.insert(17 + index as i64, id(line_type.id));
        }
    }
    for (index, font) in doc.fonts.iter().enumerate() {
        if original.fonts.iter().any(|before| before.id == font.id) {
            editor.graph.fonts.insert(1 + index as i64, id(font.id));
        }
    }
    for entity in file.entities.values() {
        if let Some(args) = entity.args("LENGTH_MEASURE_WITH_UNIT") {
            let width = match args.first() {
                Some(P21Value::Typed(_, inner)) => match inner.as_ref() {
                    P21Value::Number(value) => Some(*value),
                    _ => None,
                },
                Some(P21Value::Number(value)) => Some(*value),
                _ => None,
            };
            if let Some(width) = width {
                editor.graph.widths.entry(width.to_bits()).or_insert(id(entity.id));
            }
        }
        if editor.graph.length_unit.is_none() && entity.has("LENGTH_UNIT") && entity.has("SI_UNIT") {
            editor.graph.length_unit = Some(id(entity.id));
        }
        if editor.graph.source.is_none() && entity.has("EXTERNAL_SOURCE") {
            editor.graph.source = Some(id(entity.id));
        }
    }
    // 複合曲線の番号 → COMPOSITE_CURVE、複合図形の名前 → SYMBOL_REPRESENTATION_MAP
    for (index, curve) in doc.composite_curves.iter().enumerate() {
        if original.composite_curves.iter().any(|before| before.feature.id == curve.feature.id) {
            editor.graph.composite.insert(index as i64 + 1, id(curve.feature.id));
        }
    }
    let maps = representation_maps(file);
    for figure in doc.sub_figures.values() {
        let kept = original.sub_figures.values().any(|before| before.feature.id == figure.feature.id);
        if let Some(map) = maps.get(&id(figure.feature.id)).filter(|_| kept) {
            editor.graph.maps.insert(figure.feature.name.clone(), *map);
        }
    }
}

/// DRAUGHTING_SUBFIGURE_REPRESENTATION → それを写す SYMBOL_REPRESENTATION_MAP（最初のもの）
fn representation_maps(file: &P21File) -> HashMap<u64, u64> {
    let mut maps = HashMap::new();
    for entity in file.entities.values() {
        if let Some(P21Value::Ref(representation)) =
            entity.args("SYMBOL_REPRESENTATION_MAP").and_then(|args| args.get(1))
        {
            maps.entry(id(*representation)).or_insert(id(entity.id));
        }
    }
    maps
}

/// 色・線種・線幅・文字フォント・レイヤの値が変わったインスタンスを書き直し、消したレイヤを消す
fn tables(editor: &mut Editor<'_, '_>) {
    let (doc, original) = (editor.doc, editor.original);
    for colour in &doc.user_colors {
        if original.user_colors.iter().any(|before| before.id == colour.id && before != colour) {
            editor.rewrite_parts(id(colour.id), |parts| {
                if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "COLOUR_RGB") {
                    args.truncate(1);
                    args.extend([colour.r, colour.g, colour.b].map(|value| P21Value::Number(value / 255.0)));
                }
            });
        }
    }
    for font in &doc.fonts {
        if original.fonts.iter().any(|before| before.id == font.id && before != font) {
            editor.rewrite_parts(id(font.id), |parts| {
                if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "EXTERNALLY_DEFINED_TEXT_FONT")
                    && !args.is_empty()
                {
                    args[0] = P21Value::Typed("IDENTIFIER".into(), Box::new(P21Value::Text(font.name.clone())));
                }
            });
        }
    }
    for width in &doc.user_widths {
        if width.id != 0.0 && original.user_widths.iter().any(|before| before.id == width.id && before != width) {
            editor.rewrite_parts(id(width.id), |parts| {
                if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "LENGTH_MEASURE_WITH_UNIT")
                    && !args.is_empty()
                {
                    args[0] =
                        P21Value::Typed("POSITIVE_LENGTH_MEASURE".into(), Box::new(P21Value::Number(width.width)));
                }
            });
        }
    }
    for (index, line_type) in doc.user_line_types.iter().enumerate() {
        if original.user_line_types.iter().any(|before| before.id == line_type.id && before != line_type) {
            // 線分と空白の対を作り直し、CURVE_STYLE_FONT の番号は元のまま
            let code = 17 + index as i64;
            let start = editor.graph.instances.len();
            editor.graph.line_types.remove(&code);
            let new = editor.graph.line_type(code as f64);
            editor.adopt(start, new, id(line_type.id));
            editor.graph.line_types.insert(code, id(line_type.id));
        }
    }
    for layer in &doc.layers {
        let Some(before) = original.layers.iter().find(|before| before.id == layer.id) else {
            continue;
        };
        if before.name != layer.name {
            editor.rewrite_parts(id(layer.id), |parts| {
                if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "PRESENTATION_LAYER_ASSIGNMENT")
                    && !args.is_empty()
                {
                    args[0] = P21Value::Text(layer.name.clone());
                }
            });
        }
        if before.visible != layer.visible {
            if layer.visible {
                editor.show.insert(id(layer.id));
            } else {
                editor.hide.push(id(layer.id));
            }
        }
    }
    for before in &original.layers {
        if !doc.layers.iter().any(|layer| layer.id == before.id) {
            editor.dead.insert(id(before.id));
        }
    }
}

/// 複合曲線。変わった複合曲線は作り直し、COMPOSITE_CURVE と線の見え方の occurrence の番号を元のままにする
fn composite_curves(editor: &mut Editor<'_, '_>) {
    let (doc, original, roots) = (editor.doc, editor.original, editor.roots);
    let before: HashMap<u64, &CompositeCurve> =
        original.composite_curves.iter().map(|curve| (curve.feature.id.to_bits(), curve)).collect();
    let mut kept = HashSet::new();
    for (index, curve) in doc.composite_curves.iter().enumerate() {
        let number = index as i64 + 1;
        let Some(previous) = before.get(&curve.feature.id.to_bits()) else {
            editor.graph.composite_curve(number, curve);
            continue;
        };
        kept.insert(curve.feature.id.to_bits());
        editor.ids.push((curve.feature.id, curve.feature.id));
        if *previous == curve {
            continue;
        }
        let start = editor.graph.instances.len();
        let invisible = editor.graph.invisible.len();
        let new = editor.graph.composite_curve(number, curve);
        // 作った occurrence（最後に作った ANNOTATION_CURVE_OCCURRENCE）
        let occurrence = editor.graph.instances[start..].iter().rev().find_map(|(instance, body)| {
            matches!(body, Body::Complex(parts) if parts.iter().any(|part| part.name == "ANNOTATION_CURVE_OCCURRENCE"))
                .then_some(*instance)
        });
        editor.graph.invisible.truncate(invisible);
        let geometry = id(curve.feature.id);
        editor.adopt(start, new, geometry);
        if let (Some(occurrence), Some(old)) = (occurrence, roots.curve_occurrences.get(&curve.feature.id.to_bits())) {
            let old = id(*old);
            editor.adopt(start, occurrence, old);
            if curve.feature.visible {
                editor.show.insert(old);
            } else {
                editor.hide.push(old);
            }
        }
        editor.graph.composite.insert(number, geometry);
    }
    for curve in &original.composite_curves {
        if !kept.contains(&curve.feature.id.to_bits()) {
            editor.dead.insert(id(curve.feature.id));
            if let Some(occurrence) = roots.curve_occurrences.get(&curve.feature.id.to_bits()) {
                editor.dead.insert(id(*occurrence));
            }
        }
    }
}

/// 複合図形。名前・種類が変わったら名前を書き直し、要素の並びを書き換える。新しい複合図形は作る
fn sub_figures(editor: &mut Editor<'_, '_>) {
    let (doc, original, roots) = (editor.doc, editor.original, editor.roots);
    let before: HashMap<u64, &SubFigure> =
        original.sub_figures.values().map(|figure| (figure.feature.id.to_bits(), figure)).collect();
    let mut kept = HashSet::new();
    let mut new_figures = HashSet::new();
    for figure in doc.sub_figures.values() {
        let Some(previous) = before.get(&figure.feature.id.to_bits()) else {
            new_figures.insert(figure.feature.name.clone());
            continue;
        };
        kept.insert(figure.feature.id.to_bits());
        editor.ids.push((figure.feature.id, figure.feature.id));
        let representation = id(figure.feature.id);
        if previous.feature != figure.feature {
            let name = format!("{}{}", crate::p21_writer::sfig_prefix(figure.feature.flag), figure.feature.name);
            editor.rewrite_parts(representation, |parts| {
                if let Some((_, args)) =
                    parts.iter_mut().find(|(part, _)| part == "DRAUGHTING_SUBFIGURE_REPRESENTATION")
                    && !args.is_empty()
                {
                    args[0] = P21Value::Text(name);
                }
            });
        }
        let items = roots.sub_figures.get(&figure.feature.id.to_bits()).map_or(&[][..], Vec::as_slice);
        let source = source_members(items, &previous.members);
        editor.members(representation, &source, &figure.members);
    }
    let maps = representation_maps(editor.file);
    for figure in original.sub_figures.values() {
        if !kept.contains(&figure.feature.id.to_bits()) {
            editor.dead.insert(id(figure.feature.id));
            if let Some(map) = maps.get(&id(figure.feature.id)) {
                editor.dead.insert(*map);
            }
        }
    }
    for name in sub_figure_order(doc).into_iter().filter(|name| new_figures.contains(name)) {
        let figure = doc.sub_figures.get(&name).expect("並べた複合図形");
        editor.graph.sub_figure(figure);
    }
}

/// 用紙の大きさ・名前を書き直す（DRAWING_SHEET_REVISION の名前、PLANAR_BOX、DRAUGHTING_TITLE の名前）
fn resize_sheet(editor: &mut Editor<'_, '_>, sheet: u64, after: &DrawingSheet) {
    let (size_name, width, height) = match sheet_size(after.sheet_type) {
        Some((label, width, height)) => {
            let landscape = after.orient != 0.0;
            let (width, height) = if landscape { (height, width) } else { (width, height) };
            (format!("{label}_{}", if landscape { "horizontal" } else { "vertical" }), width, height)
        }
        None => ("FREE".to_string(), after.x, after.y),
    };
    editor.rewrite_parts(sheet, |parts| {
        if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "DRAWING_SHEET_REVISION")
            && !args.is_empty()
        {
            args[0] = P21Value::Text(size_name);
        }
    });
    let mut boxes = Vec::new();
    let mut titles = Vec::new();
    for entity in editor.file.entities.values() {
        if let Some(args) = entity.args("PRESENTATION_SIZE")
            && args.first() == Some(&P21Value::Ref(sheet as f64))
            && let Some(P21Value::Ref(planar_box)) = args.get(1)
        {
            boxes.push(id(*planar_box));
        }
        if entity.has("DRAUGHTING_TITLE") {
            titles.push(id(entity.id));
        }
    }
    for planar_box in boxes {
        editor.rewrite_parts(planar_box, |parts| {
            if let Some((_, args)) = parts.iter_mut().find(|(name, _)| name == "PLANAR_BOX")
                && args.len() >= 3
            {
                args[1] = P21Value::Number(width);
                args[2] = P21Value::Number(height);
            }
        });
    }
    let name = if after.name.is_empty() { " ".to_string() } else { after.name.clone() };
    for title in titles {
        editor.rewrite_parts(title, |parts| {
            if let Some((_, args)) = parts.iter_mut().find(|(part, _)| part == "DRAUGHTING_TITLE")
                && args.len() >= 3
            {
                args[2] = P21Value::Text(name.clone());
            }
        });
    }
}

/// 書き換えた後のインスタンス（元のインスタンスと新しいインスタンス）
struct Final {
    /// 強い参照
    strong: HashMap<u64, Vec<u64>>,
    /// 書き換えた要素の並び（並びを持つインスタンス → 要素）
    lists: HashMap<u64, Vec<u64>>,
}

impl Final {
    /// 要素の並びを持つインスタンスなら、その並び
    fn list<'f>(&'f mut self, file: &P21File, instance: u64) -> Option<&'f mut Vec<u64>> {
        match self.lists.entry(instance) {
            std::collections::hash_map::Entry::Occupied(entry) => Some(entry.into_mut()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let entity = file.entities.get(instance as f64)?;
                let (name, args) = entity.parts.iter().find(|(name, _)| container_argument(name).is_some())?;
                let mut items = Vec::new();
                if let Some(value) = args.get(container_argument(name)?) {
                    references(value, &mut items);
                }
                Some(entry.insert(items))
            }
        }
    }
}

/// 参照を辿って残すインスタンスを決め、元のファイルを書き換える
fn finish(mut editor: Editor<'_, '_>, sheet: u64, bytes: &[u8]) -> SxfWritten {
    let file = editor.file;
    let doc = editor.doc;
    let mut notices = std::mem::take(&mut editor.notices);

    // 新しい要素をレイヤの割り当てに入れる。元の割り当ては図面のレイヤの番号のインスタンス、なければ作る
    let mut assignments: HashMap<i64, u64> = HashMap::new();
    for (index, layer) in doc.layers.iter().enumerate() {
        if file.entities.get(layer.id).is_some_and(|entity| entity.has("PRESENTATION_LAYER_ASSIGNMENT")) {
            assignments.insert(index as i64 + 1, id(layer.id));
        }
    }
    let dummy = file.entities.values().find(|entity| {
        matches!(entity.args("PRESENTATION_LAYER_ASSIGNMENT").and_then(|args| args.first()), Some(P21Value::Text(name)) if name.starts_with("$$SXF_dummy"))
    });
    if let Some(dummy) = dummy {
        assignments.insert(0, id(dummy.id));
    }
    let mut assigned: HashMap<u64, Vec<u64>> = HashMap::new();
    for (code, items) in std::mem::take(&mut editor.graph.layer_items) {
        if let Some(assignment) = assignments.get(&code) {
            assigned.entry(*assignment).or_default().extend(items);
            continue;
        }
        let (name, layer) = match code {
            0 => (DUMMY_LAYER.to_string(), None),
            _ => {
                let layer = &doc.layers[(code - 1) as usize];
                (layer.name.clone(), Some(layer))
            }
        };
        let assignment =
            editor.graph.add("PRESENTATION_LAYER_ASSIGNMENT", vec![V::Str(name), V::str(" "), V::refs(&items)]);
        editor.graph.add("PRESENTATION_LAYER_USAGE", vec![V::Ref(assignment), V::Ref(sheet)]);
        if let Some(layer) = layer {
            editor.ids.push((layer.id, assignment as f64));
            if !layer.visible {
                editor.hide.push(assignment);
            }
        }
    }
    let invisible = std::mem::take(&mut editor.graph.invisible);
    editor.hide.extend(invisible);

    // 新しいインスタンスと、書き換えた後の強い参照
    let new: Vec<(u64, Body)> = std::mem::take(&mut editor.graph.instances);
    let mut state = Final { strong: HashMap::new(), lists: HashMap::new() };
    for entity in file.entities.values() {
        state.strong.insert(id(entity.id), strong_references(&entity.parts));
    }
    for (instance, rewrite) in &editor.rewritten {
        let refs = match rewrite {
            Rewrite::Body(body) => body_references(body),
            Rewrite::Parts(parts) => strong_references(parts),
        };
        state.strong.insert(*instance, refs);
    }
    let mut new_referenced = HashSet::new();
    for (instance, body) in &new {
        let refs = body_references(body);
        new_referenced.extend(refs.iter().copied());
        state.strong.insert(*instance, refs);
    }
    for (list, edit) in std::mem::take(&mut editor.lists) {
        if edit.removed.is_empty() && edit.inserted.is_empty() {
            continue;
        }
        let Some(items) = state.list(file, list) else {
            continue;
        };
        let mut out: Vec<u64> =
            edit.inserted.iter().filter(|(after, _)| after.is_none()).map(|(_, root)| *root).collect();
        for item in items.iter() {
            if !edit.removed.contains(item) {
                out.push(*item);
            }
            out.extend(edit.inserted.iter().filter(|(after, _)| *after == Some(*item)).map(|(_, root)| *root));
        }
        *items = out;
    }

    // 消すインスタンスを参照するインスタンスも消す（要素の並びからは外すだけ）
    let mut referrers: HashMap<u64, Vec<u64>> = HashMap::new();
    for (instance, refs) in &state.strong {
        for target in refs {
            referrers.entry(*target).or_default().push(*instance);
        }
    }
    let mut dead = std::mem::take(&mut editor.dead);
    let mut queue: Vec<u64> = dead.iter().copied().collect();
    while let Some(instance) = queue.pop() {
        for referrer in referrers.get(&instance).cloned().unwrap_or_default() {
            if dead.contains(&referrer) {
                continue;
            }
            if let Some(items) = state.list(file, referrer) {
                items.retain(|item| *item != instance);
                continue;
            }
            dead.insert(referrer);
            queue.push(referrer);
        }
    }
    let lists: Vec<(u64, Vec<u64>)> = state.lists.iter().map(|(list, items)| (*list, items.clone())).collect();
    for (list, items) in lists {
        // 要素の並びのほかの参照（表現の文脈など）は元のまま
        let mut refs = items;
        if let Some(entity) = file.entities.get(list as f64) {
            for (name, args) in &entity.parts {
                for (position, value) in args.iter().enumerate() {
                    if container_argument(name) != Some(position) && weak_argument(name) != Some(position) {
                        references(value, &mut refs);
                    }
                }
            }
        }
        state.strong.insert(list, refs);
    }

    // 起点（どこからも参照されていなかった元のインスタンスと、新しいインスタンス）から辿れるものを残す
    let mut referenced: HashSet<u64> = HashSet::new();
    for entity in file.entities.values() {
        referenced.extend(strong_references(&entity.parts));
    }
    let mut stack: Vec<u64> =
        file.entities.values().map(|entity| id(entity.id)).filter(|instance| !referenced.contains(instance)).collect();
    stack.extend(new.iter().map(|(instance, _)| *instance).filter(|instance| !new_referenced.contains(instance)));
    let mut live: HashSet<u64> = HashSet::new();
    while let Some(instance) = stack.pop() {
        if dead.contains(&instance) || !live.insert(instance) {
            continue;
        }
        stack.extend(state.strong.get(&instance).into_iter().flatten().filter(|target| !live.contains(*target)));
    }

    // レイヤの割り当てと非表示の指定を、残すインスタンスにする。値を変えた要素は元のレイヤから外し、新しく入れ直す
    let mut writer = StepWriter::default();
    let mut texts: HashMap<u64, String> = HashMap::new();
    let mut empty_layers = 0;
    let mut hidden = false;
    let weak_holders: Vec<&P21Entity> = file
        .entities
        .values()
        .filter(|entity| live.contains(&id(entity.id)))
        .filter(|entity| entity.has("PRESENTATION_LAYER_ASSIGNMENT") || entity.has("INVISIBILITY"))
        .collect();
    for entity in weak_holders {
        let instance = id(entity.id);
        let layer = entity.has("PRESENTATION_LAYER_ASSIGNMENT");
        let name = if layer { "PRESENTATION_LAYER_ASSIGNMENT" } else { "INVISIBILITY" };
        let (mut parts, renamed) = match editor.rewritten.remove(&instance) {
            Some(Rewrite::Parts(parts)) => (parts, true),
            _ => (entity.parts.clone(), false),
        };
        let Some((_, args)) = parts.iter_mut().find(|(part, _)| part == name) else {
            continue;
        };
        let position = weak_argument(name).unwrap_or(0);
        let mut before = Vec::new();
        if let Some(value) = args.get(position) {
            references(value, &mut before);
        }
        // 元のファイルにないインスタンスへの参照は元のまま残す（消したインスタンスだけを外す）
        let exists = |item: &u64| file.entities.get(*item as f64).is_some() || state.strong.contains_key(item);
        let mut readded = assigned.remove(&instance).unwrap_or_default();
        let mut after: Vec<u64> = before
            .iter()
            .copied()
            .filter(|item| live.contains(item) || !exists(item))
            .filter(|item| {
                if layer && editor.unassigned.contains(item) {
                    // 値を変えた要素が同じレイヤに入り直すなら、元の位置のまま
                    let again = readded.contains(item);
                    readded.retain(|candidate| candidate != item);
                    again
                } else {
                    !(!layer && editor.show.contains(item))
                }
            })
            .collect();
        if layer {
            after.extend(readded);
        } else if !hidden {
            // 非表示にするものは最初の INVISIBILITY に足す
            after.extend(editor.hide.iter().copied().filter(|item| !before.contains(item)));
            hidden = true;
        }
        if after.is_empty() {
            // 要素のないレイヤの割り当て・非表示の指定は消し、それを使う PRESENTATION_LAYER_USAGE も消す
            empty_layers += usize::from(layer);
            live.remove(&instance);
            for referrer in referrers.get(&instance).into_iter().flatten() {
                live.remove(referrer);
            }
            continue;
        }
        if after != before || renamed {
            if let Some(value) = args.get_mut(position) {
                *value = P21Value::List(after.iter().map(|item| P21Value::Ref(*item as f64)).collect());
            }
            texts.insert(instance, writer.read_instance(instance, &parts));
        }
    }
    let mut new = new;
    if !hidden && !editor.hide.is_empty() {
        let instance = editor.graph.next;
        new.push((instance, Body::Simple(part("INVISIBILITY", vec![V::refs(&editor.hide)]))));
        live.insert(instance);
    }
    if empty_layers > 0 {
        notices.add_count(
            NoticeLevel::Info,
            "empty-layer",
            "要素がなくなったレイヤの割り当てを消しました",
            empty_layers as u64,
        );
    }

    // 書き直す元のインスタンス（要素の並びを書き換えたものを含む）
    for (instance, rewrite) in &editor.rewritten {
        if live.contains(instance) && !state.lists.contains_key(instance) {
            let text = match rewrite {
                Rewrite::Body(body) => writer.instance(*instance, body),
                Rewrite::Parts(parts) => writer.read_instance(*instance, parts),
            };
            texts.insert(*instance, text);
        }
    }
    for (list, items) in &state.lists {
        let Some(entity) = file.entities.get(*list as f64).filter(|_| live.contains(list)) else {
            continue;
        };
        let live_items: Vec<u64> = items.iter().copied().filter(|item| live.contains(item)).collect();
        let mut before = Vec::new();
        if let Some((name, args)) = entity.parts.iter().find(|(name, _)| container_argument(name).is_some())
            && let Some(value) = container_argument(name).and_then(|position| args.get(position))
        {
            references(value, &mut before);
        }
        if live_items == before && !editor.rewritten.contains_key(list) {
            continue;
        }
        let mut parts = match editor.rewritten.get(list) {
            Some(Rewrite::Parts(parts)) => parts.clone(),
            _ => entity.parts.clone(),
        };
        if let Some((name, args)) = parts.iter_mut().find(|(name, _)| container_argument(name).is_some())
            && let Some(value) = container_argument(name).and_then(|position| args.get_mut(position))
        {
            *value = P21Value::List(
                items.iter().filter(|item| live.contains(*item)).map(|item| P21Value::Ref(*item as f64)).collect(),
            );
        }
        texts.insert(*list, writer.read_instance(*list, &parts));
    }

    // 元のファイルを書き換える
    let mut splices = Splices::default();
    for span in &file.spans {
        let Some(instance) = span.id.map(id) else {
            continue;
        };
        if !live.contains(&instance) {
            splices.delete(span.start, skip_space(bytes, span.end));
        } else if let Some(text) = texts.get(&instance) {
            splices.replace(span.start, span.end, text.clone().into_bytes());
        }
    }
    let mut added = String::new();
    new.sort_by_key(|(instance, _)| *instance);
    for (instance, body) in &new {
        if live.contains(instance) {
            added.push_str(&writer.instance(*instance, body));
            added.push('\n');
        }
    }
    if !added.is_empty() {
        splices.insert(file.data_end, with_line_ending(added.as_bytes(), line_ending(bytes)));
    }
    editor.graph.take_notices(&mut notices);
    if writer.nonfinite > 0 {
        notices.add_count(NoticeLevel::Warning, "invalid-number", "数でない値を 0 にして書きました", writer.nonfinite);
    }
    let mut ids = editor.ids;
    ids.extend(editor.graph.ids);
    SxfWritten { bytes: splices.apply(bytes), notices: notices.into_vec(), ids }
}
