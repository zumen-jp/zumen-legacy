//! 元の SFC を書き換えて、図面（`SxfDocument`）と違うフィーチャだけを書き直す。変えていないフィーチャは、
//! 囲み（/*SXF … SXF*/）ごと元の文字列のまま残す（docs/formats/sxf.md「Editing SFC in place」）。
//!
//! 元のファイルのフィーチャがどの定義に属するかは、読み取り（document.rs の assemble_document）と同じ規則
//! （附属書 SFC 編 1-3: 複合曲線定義・複合図形定義・用紙は、直前の定義以降のフィーチャを構成要素にする）で決め、
//! 定義（表・複合曲線・複合図形・用紙）ごとにインスタンス番号で図面と対応させる。
//!
//! - 値が同じフィーチャは元の文字列のまま。値が変わったフィーチャは、同じ番号のまま同じ位置で書き直す。
//! - 図面にないフィーチャは、後ろの空白ごと消す。複合曲線・複合図形を消すときは、その構成要素も消す。
//! - 新しいフィーチャは、属する定義の構成要素の並びの中（次に残るフィーチャの前、なければ定義のフィーチャの前）に入れる。
//!   番号は、元のファイルで使われていなければ図面の番号、使われていれば元の最大の番号より大きい 10 の倍数。
//! - 新しい複合曲線・複合図形は、構成要素と定義をまとめて、直前の定義の後（読み取りが構成要素を集め始める位置）に入れる。
//!   複合曲線の番号（出現順）が変わらないよう、図面の並びで直前の複合曲線の後に入れる。
//! - 新しい表の項目（レイヤ・色・線種・線幅・文字フォント）は、同じ種類の最後の項目の後に入れる（コードは出現順）。
//! - 読めない・対応していないフィーチャ、既定義の色・線種・線幅の宣言、置き換えられた定義は、元のまま残す。
//! - 入れるフィーチャの改行と、フィーチャの間の空白は、元のファイルに合わせる。

use crate::document::{SxfDrawing, is_curve};
use crate::features::{SxfFeature, feature_from_record};
use crate::sfc_reader::{SfcRecord, read_sfc};
use crate::splice::{Splices, line_ending, skip_space, with_line_ending};
use crate::tables::predefined_width_code;
use crate::write::{Definition, Param, SxfWritten, definition, drawable, record, sub_figure_order};
use std::collections::{HashMap, HashSet, VecDeque};
use zumen_core::{NoticeLevel, NoticeList};

/// 元のファイルを、読み取りと同じ規則で定義ごとに分けたもの（値はレコードの添字）
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Layout {
    pub layers: Vec<usize>,
    pub colours: Vec<usize>,
    pub line_types: Vec<usize>,
    /// 既定義の値でない線幅だけ（既定義の値の線幅は図面に入らない）
    pub widths: Vec<usize>,
    pub fonts: Vec<usize>,
    /// 最後の図面表題欄（読み取りは後のものを使う）
    pub title: Option<usize>,
    /// 最初の用紙（2 つ目以降は無視される）
    pub sheet: Option<usize>,
    pub sheet_members: Vec<usize>,
    /// 複合曲線定義と構成要素（複合曲線に使えない要素は除く）
    pub composites: Vec<(usize, Vec<usize>)>,
    /// 名前ごとの複合図形定義と構成要素。同じ名前の後の定義が値を置き換え、並びは最初の定義の位置
    pub sub_figures: Vec<(String, usize, Vec<usize>)>,
    /// 最後の複合図形定義
    pub last_sub_figure: Option<usize>,
}

/// 読み取り（assemble_document）と同じ規則で、フィーチャを定義ごとに分ける
pub(crate) fn layout(features: &[Option<SxfFeature>]) -> Layout {
    let mut layout = Layout::default();
    let mut pending: Vec<usize> = Vec::new();
    for (index, feature) in features.iter().enumerate() {
        let Some(feature) = feature else {
            continue;
        };
        match feature {
            SxfFeature::Layer(_) => layout.layers.push(index),
            SxfFeature::UserDefinedColour(_) => layout.colours.push(index),
            SxfFeature::UserDefinedFont(_) => layout.line_types.push(index),
            SxfFeature::Width(width) => {
                if predefined_width_code(width.width) == 0 {
                    layout.widths.push(index);
                }
            }
            SxfFeature::TextFont(_) => layout.fonts.push(index),
            SxfFeature::DrawingAttribute(_) => layout.title = Some(index),
            SxfFeature::PreDefinedFont { .. } | SxfFeature::PreDefinedColour { .. } => {}
            SxfFeature::CompositeCurveOrg(_) => {
                let members = std::mem::take(&mut pending)
                    .into_iter()
                    .filter(|member| features[*member].as_ref().is_some_and(|member| is_curve(member.kind())))
                    .collect();
                layout.composites.push((index, members));
            }
            SxfFeature::SfigOrg(figure) => {
                let members = std::mem::take(&mut pending);
                match layout.sub_figures.iter_mut().find(|(name, ..)| *name == figure.name) {
                    Some(entry) => {
                        entry.1 = index;
                        entry.2 = members;
                    }
                    None => layout.sub_figures.push((figure.name.clone(), index, members)),
                }
                layout.last_sub_figure = Some(index);
            }
            SxfFeature::DrawingSheet(_) => {
                if layout.sheet.is_none() {
                    layout.sheet = Some(index);
                    layout.sheet_members = std::mem::take(&mut pending);
                }
            }
            _ => pending.push(index),
        }
    }
    layout.sheet_members.extend(pending);
    layout
}

/// 元の SFC のレコードと、その値
pub(crate) struct Source<'a> {
    pub bytes: &'a [u8],
    pub records: Vec<SfcRecord>,
    pub features: Vec<Option<SxfFeature>>,
}

impl<'a> Source<'a> {
    pub(crate) fn read(bytes: &'a [u8]) -> Self {
        let records = read_sfc(bytes).records;
        let features = records.iter().map(|record| feature_from_record(record).ok().flatten()).collect();
        Source { bytes, records, features }
    }
}

/// 前から順に、同じインスタンス番号のものを対応させた結果
struct Pairing {
    /// 図面の項目ごとの、対応する元のレコード
    matched: Vec<Option<usize>>,
    /// 図面に対応するもののない元のレコード（消す）
    unmatched: Vec<usize>,
}

/// 元の並び（レコードの添字）と図面の並びを、インスタンス番号で前から対応させる。並びの順が入れ替わった項目は
/// 対応させず、元のレコードを消して入れ直す
fn pair(records: &[SfcRecord], source: &[usize], ids: impl Iterator<Item = f64>) -> Pairing {
    let mut by_id: HashMap<u64, VecDeque<usize>> = HashMap::new();
    for (position, index) in source.iter().enumerate() {
        by_id.entry(records[*index].id.to_bits()).or_default().push_back(position);
    }
    let mut taken = vec![false; source.len()];
    let mut last: Option<usize> = None;
    let matched = ids
        .map(|id| {
            let position = by_id.get_mut(&id.to_bits())?.pop_front()?;
            if last.is_some_and(|last| position < last) {
                return None;
            }
            taken[position] = true;
            last = Some(position);
            Some(source[position])
        })
        .collect();
    let unmatched = source.iter().zip(&taken).filter(|(_, taken)| !**taken).map(|(index, _)| *index).collect();
    Pairing { matched, unmatched }
}

/// 構成要素の並びの対応
struct Members<'d> {
    pairing: Pairing,
    current: &'d [SxfFeature],
    /// 新しい構成要素を入れる既定の位置（定義のフィーチャの前）
    anchor: Anchor,
}

/// 新しいフィーチャを入れる位置
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// レコードの前
    Before(usize),
    /// レコードの後（後ろの空白の後）
    After(usize),
    /// DATA 節の初め
    Start,
    /// DATA 節の終わり（ENDSEC; の前）
    End,
}

struct Editor<'a, 'd> {
    source: &'a Source<'a>,
    drawing: &'d SxfDrawing,
    splices: Splices,
    notices: NoticeList,
    ending: &'static [u8],
    /// フィーチャの間の空白
    separator: Vec<u8>,
    data_start: usize,
    data_end: usize,
    /// 書いた後に残るインスタンス番号
    used: HashSet<u64>,
    next: f64,
    ids: Vec<(f64, f64)>,
    skipped: u64,
}

/// 元のファイルを書き換えられず、新しく書いたことの知らせ
pub(crate) fn rewrite_notice(format: &str) -> zumen_core::Notice {
    zumen_core::Notice {
        level: NoticeLevel::Warning,
        code: "rewrite".into(),
        message: format!("元の {format} を読み直し切れない（読み取りの上限）ため、新しく書きました"),
        count: 1,
    }
}

/// 数を 10 の倍数に切り上げる
fn round_up(value: f64) -> f64 {
    (value / 10.0).ceil() * 10.0
}

impl Editor<'_, '_> {
    fn position(&self, anchor: Anchor) -> usize {
        match anchor {
            Anchor::Before(index) => self.source.records[index].span.0,
            Anchor::After(index) => skip_space(self.source.bytes, self.source.records[index].span.1),
            Anchor::Start => self.data_start,
            Anchor::End => self.data_end,
        }
    }

    /// レコードの文字列。改行はファイルに合わせ、最後の改行を除く
    fn text(&mut self, id: f64, name: &str, version: &str, params: Vec<Param>) -> Vec<u8> {
        let mut text = record(id, name, version, params, &mut self.notices);
        if text.last() == Some(&b'\n') {
            text.pop();
        }
        with_line_ending(&text, self.ending)
    }

    /// 新しいフィーチャのインスタンス番号。図面の番号が使われていなければそのまま、使われていれば振り直す
    fn fresh(&mut self, wanted: f64) -> f64 {
        let id = if wanted > 0.0 && wanted.fract() == 0.0 && !self.used.contains(&wanted.to_bits()) {
            wanted
        } else {
            let id = self.next;
            self.next += 10.0;
            id
        };
        self.used.insert(id.to_bits());
        self.ids.push((wanted, id));
        id
    }

    fn insert(&mut self, anchor: Anchor, text: Vec<u8>) {
        let mut bytes = text;
        bytes.extend_from_slice(&self.separator);
        let at = self.position(anchor);
        self.splices.insert(at, bytes);
    }

    fn delete(&mut self, index: usize) {
        let (start, end) = self.source.records[index].span;
        let end = skip_space(self.source.bytes, end);
        self.splices.delete(start, end);
    }

    /// 囲みの版。元の囲みが求める版より新しければ元のまま
    fn version(&self, index: usize, required: &'static str) -> &'static str {
        match (self.source.records[index].version, required) {
            ("3.1", _) => "3.1",
            ("3", _) => "3",
            (_, required) => required,
        }
    }

    fn rewrite(&mut self, index: usize, name: &str, version: &'static str, params: Vec<Param>) {
        let id = self.source.records[index].id;
        let version = self.version(index, version);
        let text = self.text(id, name, version, params);
        let (start, end) = self.source.records[index].span;
        self.splices.replace(start, end, text);
    }

    /// 要素の文字列を書く（定義のフィーチャは要素として書けないので数える）
    fn member_text(&mut self, feature: &SxfFeature) -> Option<Vec<u8>> {
        let Some((name, version, params)) = drawable(feature) else {
            self.skipped += 1;
            return None;
        };
        let id = self.fresh(feature.id());
        Some(self.text(id, name, version, params))
    }

    fn definition_text(&mut self, id: f64, item: Definition<'_>) -> Vec<u8> {
        let (name, version, params) = definition(item);
        let id = self.fresh(id);
        self.text(id, name, version, params)
    }

    /// 構成要素の並びを書き換える
    fn members(&mut self, members: Members<'_>) {
        let Members { pairing, current, anchor } = members;
        for index in &pairing.unmatched {
            self.delete(*index);
        }
        // 新しい構成要素は、後ろで最初に残る構成要素の前に入れる。後ろに残るものがなければ、前で最後に残る構成要素の
        // 後（用紙より後に書かれた要素の後にも足せる）。どちらもなければ定義のフィーチャの前
        let mut anchors = vec![anchor; current.len()];
        let mut next = None;
        for (position, matched) in pairing.matched.iter().enumerate().rev() {
            anchors[position] = next.unwrap_or(anchor);
            if let Some(index) = matched {
                next = Some(Anchor::Before(*index));
            }
        }
        let mut previous = None;
        for (position, matched) in pairing.matched.iter().enumerate() {
            if let Some(index) = matched {
                previous = Some(*index);
            } else if anchors[position] == anchor
                && let Some(index) = previous
            {
                anchors[position] = Anchor::After(index);
            }
        }
        for (position, feature) in current.iter().enumerate() {
            match pairing.matched[position] {
                Some(index) => {
                    self.ids.push((feature.id(), feature.id()));
                    if self.source.features[index].as_ref() != Some(feature) {
                        match drawable(feature) {
                            Some((name, version, params)) => self.rewrite(index, name, version, params),
                            None => {
                                self.skipped += 1;
                                self.delete(index);
                            }
                        }
                    }
                }
                None => {
                    if let Some(text) = self.member_text(feature) {
                        self.insert(anchors[position], text);
                    }
                }
            }
        }
    }

    /// 表（レイヤ・色・線種・線幅・文字フォント）の並びを書き換える。新しい項目は同じ種類の最後の項目の後。
    /// wrap は値をフィーチャにする（元のレコードの値と比べる）
    fn table<T>(
        &mut self,
        source: &[usize],
        current: &[T],
        id: impl Fn(&T) -> f64,
        wrap: impl Fn(&T) -> SxfFeature,
        params: impl Fn(&T) -> (&'static str, &'static str, Vec<Param>),
    ) {
        let pairing = pair(&self.source.records, source, current.iter().map(&id));
        for index in &pairing.unmatched {
            self.delete(*index);
        }
        let mut anchor = source.last().map_or(Anchor::Start, |index| Anchor::After(*index));
        for (position, value) in current.iter().enumerate() {
            match pairing.matched[position] {
                Some(index) => {
                    self.ids.push((id(value), id(value)));
                    if self.source.features[index].as_ref() != Some(&wrap(value)) {
                        let (name, version, params) = params(value);
                        self.rewrite(index, name, version, params);
                    }
                    anchor = Anchor::After(index);
                }
                None => {
                    let (name, version, params) = params(value);
                    let id = self.fresh(id(value));
                    let text = self.text(id, name, version, params);
                    self.insert(anchor, text);
                }
            }
        }
    }
}

/// 元の SFC（source）を書き換えて、図面を書く
pub(crate) fn write_sfc_edit(drawing: &SxfDrawing, bytes: &[u8]) -> SxfWritten {
    let source = Source::read(bytes);
    // 読み取りの上限（zumen_core::limits）で元のファイルを読み直し切れなければ、書き換えられないので新しく書く
    if zumen_core::limits::stopped() {
        let mut written = crate::write::write_sfc_new(drawing);
        written.notices.insert(0, rewrite_notice("SFC"));
        return written;
    }
    let layout = layout(&source.features);
    // DATA 節の初め（最初のフィーチャの前）と終わり（最後のフィーチャの後の ENDSEC; の前）
    let data = bytes.windows(5).position(|window| window == b"DATA;").map_or(0, |at| at + 5);
    let data_start = source.records.first().map_or(skip_space(bytes, data), |record| record.span.0);
    let last = source.records.last().map_or(data_start, |record| record.span.1);
    let data_end = bytes[last..].windows(7).position(|window| window == b"ENDSEC;").map_or(bytes.len(), |at| last + at);
    let ending = line_ending(bytes);
    // フィーチャの間の空白は、最初の 2 つのフィーチャの間に合わせる
    let separator = match (source.records.first(), source.records.get(1)) {
        (Some(first), Some(second)) if bytes[first.span.1..second.span.0].iter().all(u8::is_ascii_whitespace) => {
            bytes[first.span.1..second.span.0].to_vec()
        }
        _ => ending.to_vec(),
    };
    let mut used: HashSet<u64> = source.records.iter().map(|record| record.id.to_bits()).collect();
    let max = source.records.iter().map(|record| record.id).fold(0.0, f64::max);
    used.insert(0f64.to_bits());
    let mut editor = Editor {
        source: &source,
        drawing,
        splices: Splices::default(),
        notices: NoticeList::new(),
        ending,
        separator,
        data_start,
        data_end,
        used,
        next: round_up(max + 1.0),
        ids: Vec::new(),
        skipped: 0,
    };
    edit(&mut editor, &layout);
    let doc_notices = std::mem::take(&mut editor.notices);
    let mut notices = doc_notices;
    if editor.skipped > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "not-drawable",
            "要素の並びにある定義のフィーチャ（レイヤ・用紙など）は書いていません",
            editor.skipped,
        );
    }
    SxfWritten { bytes: editor.splices.apply(bytes), notices: notices.into_vec(), ids: editor.ids }
}

fn edit(editor: &mut Editor<'_, '_>, layout: &Layout) {
    let doc = &editor.drawing.document;
    let records = &editor.source.records;
    // 表
    editor.table(
        &layout.colours,
        &doc.user_colors,
        |value| value.id,
        |value| SxfFeature::UserDefinedColour(value.clone()),
        |value| definition(Definition::Colour(value)),
    );
    editor.table(
        &layout.line_types,
        &doc.user_line_types,
        |value| value.id,
        |value| SxfFeature::UserDefinedFont(value.clone()),
        |value| definition(Definition::LineType(value)),
    );
    editor.table(
        &layout.widths,
        &doc.user_widths,
        |value| value.id,
        |value| SxfFeature::Width(value.clone()),
        |value| definition(Definition::Width(value)),
    );
    editor.table(
        &layout.fonts,
        &doc.fonts,
        |value| value.id,
        |value| SxfFeature::TextFont(value.clone()),
        |value| definition(Definition::Font(value)),
    );
    editor.table(
        &layout.layers,
        &doc.layers,
        |value| value.id,
        |value| SxfFeature::Layer(value.clone()),
        |value| definition(Definition::Layer(value)),
    );

    // 複合曲線。番号は出現順なので、並びの順に対応させる
    let sources: Vec<usize> = layout.composites.iter().map(|(index, _)| *index).collect();
    let members_of: HashMap<usize, &Vec<usize>> =
        layout.composites.iter().map(|(index, members)| (*index, members)).collect();
    let pairing = pair(records, &sources, doc.composite_curves.iter().map(|curve| curve.feature.id));
    for index in &pairing.unmatched {
        editor.delete(*index);
        for member in members_of[index] {
            editor.delete(*member);
        }
    }
    let mut anchor = Anchor::Start;
    for (position, curve) in doc.composite_curves.iter().enumerate() {
        match pairing.matched[position] {
            Some(index) => {
                editor.ids.push((curve.feature.id, curve.feature.id));
                if editor.source.features[index].as_ref() != Some(&SxfFeature::CompositeCurveOrg(curve.feature.clone()))
                {
                    let (name, version, params) = definition(Definition::Composite(&curve.feature));
                    editor.rewrite(index, name, version, params);
                }
                let pairing = pair(records, members_of[&index], curve.members.iter().map(SxfFeature::id));
                editor.members(Members { pairing, current: &curve.members, anchor: Anchor::Before(index) });
                anchor = Anchor::After(index);
            }
            None => {
                // 構成要素と定義をまとめて、直前の複合曲線の後に入れる
                let mut block = Vec::new();
                for member in &curve.members {
                    if let Some(text) = editor.member_text(member) {
                        block.push(text);
                    }
                }
                block.push(editor.definition_text(curve.feature.id, Definition::Composite(&curve.feature)));
                for text in block {
                    editor.insert(anchor, text);
                }
            }
        }
    }

    // 複合図形。名前で引くので並びの順は問わない
    let mut by_id: HashMap<u64, usize> = HashMap::new();
    for (position, (_, index, _)) in layout.sub_figures.iter().enumerate() {
        by_id.entry(records[*index].id.to_bits()).or_insert(position);
    }
    let mut matched_figures = HashSet::new();
    let mut new_figures = Vec::new();
    for figure in doc.sub_figures.values() {
        match by_id.get(&figure.feature.id.to_bits()).filter(|position| !matched_figures.contains(*position)) {
            Some(position) => {
                matched_figures.insert(*position);
                let (_, index, members) = &layout.sub_figures[*position];
                editor.ids.push((figure.feature.id, figure.feature.id));
                if editor.source.features[*index].as_ref() != Some(&SxfFeature::SfigOrg(figure.feature.clone())) {
                    let (name, version, params) = definition(Definition::SubFigure(&figure.feature));
                    editor.rewrite(*index, name, version, params);
                }
                let pairing = pair(records, members, figure.members.iter().map(SxfFeature::id));
                editor.members(Members { pairing, current: &figure.members, anchor: Anchor::Before(*index) });
            }
            None => new_figures.push(figure.feature.name.clone()),
        }
    }
    for (position, (_, index, members)) in layout.sub_figures.iter().enumerate() {
        if !matched_figures.contains(&position) {
            editor.delete(*index);
            for member in members {
                editor.delete(*member);
            }
        }
    }
    // 新しい複合図形は、配置される複合図形を先にして、最後の複合図形定義の後にまとめて入れる
    let anchor = layout.last_sub_figure.map_or(Anchor::Start, Anchor::After);
    let new: HashSet<String> = new_figures.into_iter().collect();
    for name in sub_figure_order(doc).into_iter().filter(|name| new.contains(name)) {
        let figure = doc.sub_figures.get(&name).expect("並べた複合図形");
        let mut block = Vec::new();
        for member in &figure.members {
            if let Some(text) = editor.member_text(member) {
                block.push(text);
            }
        }
        block.push(editor.definition_text(figure.feature.id, Definition::SubFigure(&figure.feature)));
        for text in block {
            editor.insert(anchor, text);
        }
    }

    // 用紙の要素・図面表題欄・用紙
    let sheet_anchor = match (layout.sheet, &doc.sheet) {
        (Some(index), Some(_)) => Anchor::Before(index),
        _ => Anchor::End,
    };
    let pairing = pair(records, &layout.sheet_members, doc.sheet_members.iter().map(SxfFeature::id));
    editor.members(Members { pairing, current: &doc.sheet_members, anchor: sheet_anchor });
    match (layout.title, &doc.title) {
        (Some(index), Some(title)) if records[index].id == title.id => {
            editor.ids.push((title.id, title.id));
            if editor.source.features[index].as_ref() != Some(&SxfFeature::DrawingAttribute(title.clone())) {
                let (name, version, params) = definition(Definition::Title(title));
                editor.rewrite(index, name, version, params);
            }
        }
        (index, title) => {
            if let Some(index) = index {
                editor.delete(index);
            }
            if let Some(title) = title {
                let text = editor.definition_text(title.id, Definition::Title(title));
                editor.insert(sheet_anchor, text);
            }
        }
    }
    match (layout.sheet, &doc.sheet) {
        (Some(index), Some(sheet)) => {
            editor.ids.push((sheet.id, sheet.id));
            if editor.source.features[index].as_ref() != Some(&SxfFeature::DrawingSheet(sheet.clone())) {
                let (name, version, params) = definition(Definition::Sheet(sheet));
                editor.rewrite(index, name, version, params);
            }
        }
        (Some(index), None) => editor.delete(index),
        (None, Some(sheet)) => {
            // 用紙のなかった図面に用紙を足す。最後に入れて、残りのフィーチャを構成要素にする
            let text = editor.definition_text(sheet.id, Definition::Sheet(sheet));
            editor.insert(Anchor::End, text);
        }
        (None, None) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{SubFigure, SxfDocument};
    use crate::features::FeatureKind;

    /// layout から組み立て直した図面が、読み取りの図面と同じになる（同じ規則であることを確かめる）
    fn rebuild(source: &Source<'_>, layout: &Layout) -> SxfDocument {
        let feature = |index: &usize| source.features[*index].clone().expect("読めたフィーチャ");
        let mut doc = SxfDocument::default();
        let table = |indices: &[usize]| indices.iter().map(feature).collect::<Vec<_>>();
        for value in table(&layout.layers) {
            if let SxfFeature::Layer(layer) = value {
                doc.layers.push(layer);
            }
        }
        for value in table(&layout.colours) {
            if let SxfFeature::UserDefinedColour(color) = value {
                doc.user_colors.push(color);
            }
        }
        for value in table(&layout.line_types) {
            if let SxfFeature::UserDefinedFont(line_type) = value {
                doc.user_line_types.push(line_type);
            }
        }
        for value in table(&layout.widths) {
            if let SxfFeature::Width(width) = value {
                doc.user_widths.push(width);
            }
        }
        for value in table(&layout.fonts) {
            if let SxfFeature::TextFont(font) = value {
                doc.fonts.push(font);
            }
        }
        doc.title = layout.title.map(|index| match feature(&index) {
            SxfFeature::DrawingAttribute(title) => title,
            _ => unreachable!(),
        });
        doc.sheet = layout.sheet.map(|index| match feature(&index) {
            SxfFeature::DrawingSheet(sheet) => sheet,
            _ => unreachable!(),
        });
        doc.sheet_members = table(&layout.sheet_members);
        for (index, members) in &layout.composites {
            let SxfFeature::CompositeCurveOrg(curve) = feature(index) else { unreachable!() };
            doc.composite_curves.push(crate::document::CompositeCurve { feature: curve, members: table(members) });
        }
        for (name, index, members) in &layout.sub_figures {
            let SxfFeature::SfigOrg(figure) = feature(index) else { unreachable!() };
            doc.sub_figures.set(name.clone(), SubFigure { feature: figure, members: table(members) });
        }
        doc
    }

    #[test]
    fn lays_out_records_as_the_reader_assembles_them() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/sfc");
        for name in ["features", "nosheet", "sample"] {
            let bytes = std::fs::read(root.join(format!("{name}.sfc"))).unwrap();
            let source = Source::read(&bytes);
            let mut expected = crate::parse_sfc(&bytes).unwrap().document;
            expected.counts = Vec::new();
            assert_eq!(rebuild(&source, &layout(&source.features)), expected, "{name}");
        }
        assert!(is_curve(FeatureKind::Arc) && !is_curve(FeatureKind::Circle));
    }
}
