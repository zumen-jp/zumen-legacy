//! JWW ファイルのすべて（`JwwFile`）。読み取り（reader.rs）が使わない値も持ち、書くと元と同じバイト列になる。
//!
//! - ヘッダは仕様の欄すべてを型つきで持つ（header.rs）。
//! - 図形の一覧・ブロック定義の一覧の 1 件（レコード）は、図形の値と、読んだときの本体のバイト列を持つ。
//!   図形が変わっていないレコードは元のバイト列のまま書き、変わったものと足したものだけを書き直す。
//! - クラスのタグ（CArchive のクラスとオブジェクトの通し番号）は、書くときに MFC と同じ規則で振り直す。
//!   ブロックの配置は定義を通し番号（`JwwBlockRecord::number`）で参照するので、並びが変わっても参照は変わらない。
//! - ブロック定義の一覧より後ろ（版 700 以降の同梱画像）は、そのまま書く。

use crate::archive::{ClassTag, ReadTable, Reader, Result, WriteTable, Writer, unicode};
use crate::header::{JwwHeader, strings_unicode};
use crate::reader::{
    JwwBase, JwwEntity, JwwLine, JwwPoint, JwwText, find_entity_list, is_jww, read_base, read_entity_body, read_line,
    read_text,
};
use zumen_core::{Notice, NoticeLevel, NoticeList, ReadError};

/// 書いた JWW と、形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwwWritten {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}

/// 図形の一覧の 1 件
#[derive(Debug, Clone, PartialEq)]
pub struct JwwRecord {
    pub entity: JwwEntity,
    /// 読んだときのタグと本体のバイト列（足した図形は None）
    source: Option<RecordSource>,
}

#[derive(Debug, Clone, PartialEq)]
struct RecordSource {
    class: &'static str,
    tag: ClassTag,
    body: Vec<u8>,
}

impl JwwRecord {
    /// 新しい図形のレコード
    pub fn new(entity: JwwEntity) -> Self {
        Self { entity, source: None }
    }
}

impl From<JwwEntity> for JwwRecord {
    fn from(entity: JwwEntity) -> Self {
        Self::new(entity)
    }
}

/// ブロック定義（CDataList）
#[derive(Debug, Clone, PartialEq)]
pub struct JwwBlockRecord {
    /// 図形と同じ属性（Jw_cad は使わない）
    pub base: JwwBase,
    /// 定義の通し番号。ブロックの配置（`JwwBlock::definition`）はこの番号で参照する
    pub number: u32,
    /// 実際に参照されているか
    pub referenced: bool,
    /// 定義した時刻（CTime、1970-01-01 UTC からの秒）
    pub time: u32,
    /// 名前。版 4.10 以降は後ろに "@@SfigorgFlag@@" と複合図形の種別（1〜4）が付くことがある
    pub name: String,
    pub entities: Vec<JwwRecord>,
    source: Option<BlockSource>,
}

#[derive(Debug, Clone, PartialEq)]
struct BlockSource {
    tag: ClassTag,
    /// 属性・番号・参照・時刻・名前のバイト列
    head: Vec<u8>,
}

impl JwwBlockRecord {
    /// 新しいブロック定義。属性は Jw_cad の既定（線種 1・線色 2）、時刻は 0
    pub fn new(number: u32, name: impl Into<String>, entities: Vec<JwwRecord>) -> Self {
        let base = JwwBase { pen_style: 1, pen_color: 2, ..JwwBase::default() };
        Self { base, number, referenced: false, time: 0, name: name.into(), entities, source: None }
    }
}

/// JWW ファイルのすべて
#[derive(Debug, Clone, PartialEq)]
pub struct JwwFile {
    /// 版（600、700 など）。書く版でもある
    pub version: u32,
    pub header: JwwHeader,
    /// 図形の一覧
    pub entities: Vec<JwwRecord>,
    /// ブロック定義の一覧
    pub blocks: Vec<JwwBlockRecord>,
    /// ブロック定義の一覧より後ろ（版 700 以降の同梱画像の件数と画像）。そのまま書く
    pub trailer: Vec<u8>,
    /// 読んだときの版とヘッダのバイト列（足したヘッダは None）
    source: Option<FileSource>,
}

#[derive(Debug, Clone, PartialEq)]
struct FileSource {
    version: u32,
    header: Vec<u8>,
    /// 文字列を UTF-16LE で書いたファイルか（同じ版で書くときは、足した文字列もこれに合わせる）
    unicode: bool,
}

/// 図形の本体の最小のバイト数（件数から確保する量を抑える）
const MIN_RECORD_SIZE: usize = 16;

impl JwwFile {
    /// 新しい図面（ヘッダは header、図形はなし）
    pub fn new(version: u32, header: JwwHeader) -> Self {
        let trailer = if version >= 700 { 0u32.to_le_bytes().to_vec() } else { Vec::new() };
        Self { version, header, entities: Vec::new(), blocks: Vec::new(), trailer, source: None }
    }

    /// JWW を読む。ヘッダは仕様の並びで読めたところまでを型つきで、残りはバイト列で持つ。
    /// 図形の一覧とブロック定義の一覧が終わりまで読めなければエラー（表示用の `read_jww` は途中まで読む）
    pub fn read(bytes: &[u8]) -> Result<Self> {
        if !is_jww(bytes) {
            return Err(ReadError::new("JWW ファイルではありません"));
        }
        let mut reader = Reader::new(bytes, 8);
        let version = reader.u32()?;
        // 図形の一覧の始まり。表示の読み取り（read_jww）と同じく最初の図形クラスの登録の前を先に試し、
        // 見つからない・読めない（図形の一覧が空など）ときは、ヘッダを仕様の並びで読み終えたところを試す
        let full = JwwHeader::read(bytes, bytes.len(), version)?;
        let mut starts = Vec::with_capacity(2);
        starts.extend(find_entity_list(bytes));
        let end = bytes.len() - full.rest.len();
        if !starts.contains(&end) {
            starts.push(end);
        }
        let mut error = ReadError::new("図形の一覧が見つかりません");
        for start in starts {
            let read = JwwHeader::read(bytes, start, version)
                .and_then(|header| read_body(bytes, start, version).map(|body| (header, body)));
            match read {
                Ok((header, (entities, blocks, trailer))) => {
                    let unicode = strings_unicode(bytes);
                    let source = Some(FileSource { version, header: bytes[..start].to_vec(), unicode });
                    return Ok(Self { version, header, entities, blocks, trailer, source });
                }
                Err(failure) => error = failure,
            }
        }
        Err(error)
    }

    /// JWW に書く。読んだファイルで、版と値が変わっていない部分は元のバイト列のまま書く。文字列は、読んだファイルを
    /// 同じ版で書くならそのファイルと同じ符号、ほかは版 700 以降なら UTF-16LE、それより前は Shift_JIS
    pub fn write(&self) -> JwwWritten {
        let mut notices = NoticeList::new();
        let version = self.version;
        let source = self.source.as_ref().filter(|source| source.version == version);
        let mut out = Writer::with_unicode(source.map_or_else(|| unicode(version), |source| source.unicode));
        match source {
            Some(source)
                if JwwHeader::read(&source.header, source.header.len(), version).ok().as_ref()
                    == Some(&self.header) =>
            {
                out.raw(&source.header);
            }
            _ => self.header.write(&mut out, version),
        }
        let mut context = Context { version, reuse: source.is_some(), table: WriteTable::new(), notices: &mut notices };
        context.records(&mut out, &self.entities);
        out.count(self.blocks.len());
        for block in &self.blocks {
            context.block(&mut out, block);
        }
        // 同梱画像の欄（件数の DWORD と画像）は版 700 以降。読んだファイルを同じ版で書くときはそのまま書き、版を
        // 700 より前に変えたときは書かない
        match self.source.as_ref().map(|source| source.version != version) {
            Some(false) => out.raw(&self.trailer),
            _ if version >= 700 => out.raw(if self.trailer.is_empty() { &[0; 4] } else { &self.trailer }),
            None => out.raw(&self.trailer),
            Some(true) => {
                if self.trailer.get(..4).is_some_and(|count| count != [0; 4]) {
                    notices.add(
                        NoticeLevel::Warning,
                        "embedded-image",
                        "版 700 より前の JWW は同梱画像を持たないため、書いていません（画像の配置の文字は残ります）",
                    );
                }
            }
        }
        if out.replaced > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "text-encoding",
                "Shift_JIS で表せない文字を ? にしました（版 700 より前の JWW と、文字列が Shift_JIS の読んだファイル）",
                out.replaced as u64,
            );
        }
        JwwWritten { bytes: out.bytes, notices: notices.into_vec() }
    }
}

/// 図形の一覧（とブロック定義）とその後ろを読む
fn read_body(bytes: &[u8], start: usize, version: u32) -> Result<(Vec<JwwRecord>, Vec<JwwBlockRecord>, Vec<u8>)> {
    let mut reader = Reader::new(bytes, start);
    let mut table = ReadTable::new();
    let entities = read_records(&mut reader, bytes, version, &mut table)?;
    let count = reader.count().map_err(|_| ReadError::new("ブロック定義の一覧がありません"))?;
    let mut blocks = Vec::with_capacity((count as usize).min(reader.remaining() / MIN_RECORD_SIZE));
    for _ in 0..count {
        stop_at_limit()?;
        let Some((class, _, tag)) = table.read_class(&mut reader)? else {
            continue;
        };
        if class != "CDataList" {
            return Err(ReadError::new(format!("ブロック定義の一覧に {class} があります")));
        }
        table.reserve();
        let head_start = reader.pos();
        let base = read_base(&mut reader, version)?;
        let number = reader.u32()?;
        let referenced = reader.u32()? != 0;
        let time = reader.u32()?;
        let name = reader.cstring()?;
        let head = bytes[head_start..reader.pos()].to_vec();
        let entities = read_records(&mut reader, bytes, version, &mut table)?;
        let source = Some(BlockSource { tag, head });
        blocks.push(JwwBlockRecord { base, number, referenced, time, name, entities, source });
    }
    Ok((entities, blocks, bytes[reader.pos()..].to_vec()))
}

/// 図形の一覧を読む。空のオブジェクト（NULL）は内容がないので読み飛ばす
fn read_records(reader: &mut Reader<'_>, bytes: &[u8], version: u32, table: &mut ReadTable) -> Result<Vec<JwwRecord>> {
    let count = reader.count()?;
    let mut records = Vec::with_capacity((count as usize).min(reader.remaining() / MIN_RECORD_SIZE));
    for _ in 0..count {
        stop_at_limit()?;
        let Some((class, _, tag)) = table.read_class(reader)? else {
            continue;
        };
        let start = reader.pos();
        let entity = read_entity_body(reader, &class, version)?;
        table.reserve();
        let source = RecordSource { class: class_of(&entity), tag, body: bytes[start..reader.pos()].to_vec() };
        records.push(JwwRecord { entity, source: Some(source) });
    }
    Ok(records)
}

/// 読み取りの上限（zumen_core::limits）を超えたら、途中までのファイルは書けないのでエラーにする
pub(crate) fn stop_at_limit() -> Result<()> {
    if zumen_core::limits::tick() {
        return Err(ReadError::new("読み取りの上限を超えたため、途中でやめました"));
    }
    Ok(())
}

/// 図形のクラス名
fn class_of(entity: &JwwEntity) -> &'static str {
    match entity {
        JwwEntity::Line(_) => "CDataSen",
        JwwEntity::Arc(_) => "CDataEnko",
        JwwEntity::Point(_) => "CDataTen",
        JwwEntity::Text(_) => "CDataMoji",
        JwwEntity::Solid(_) | JwwEntity::CircleSolid(_) => "CDataSolid",
        JwwEntity::Block(_) => "CDataBlock",
        JwwEntity::Dimension(_) => "CDataSunpou",
    }
}

/// クラスのスキーマ。Jw_cad は保存する版と同じ番号を書く
fn schema(version: u32) -> u16 {
    u16::try_from(version).unwrap_or(u16::MAX)
}

/// 書くときの状態
struct Context<'a> {
    version: u32,
    /// 読んだときと同じ版で書く（元のバイト列を使える）
    reuse: bool,
    table: WriteTable,
    notices: &'a mut NoticeList,
}

impl Context<'_> {
    fn records(&mut self, out: &mut Writer, records: &[JwwRecord]) {
        out.count(records.len());
        for record in records {
            self.record(out, record);
        }
    }

    fn record(&mut self, out: &mut Writer, record: &JwwRecord) {
        let class = class_of(&record.entity);
        // 元のタグと本体は、同じ版で同じクラスのときだけ使う（版を変えたら、Jw_cad と同じく版をスキーマにする）
        let source = record.source.as_ref().filter(|source| self.reuse && source.class == class);
        self.table.write_object(out, class, schema(self.version), source.map(|source| source.tag));
        if let Some(source) = source {
            let mut reader = Reader::new(&source.body, 0);
            let same = read_entity_body(&mut reader, class, self.version).ok().as_ref() == Some(&record.entity);
            if same && reader.remaining() == 0 {
                out.raw(&source.body);
                return;
            }
        }
        // 寸法の SXF のモードは図形の値にないので、元の本体から引き継ぐ
        let mode = match (&record.entity, source) {
            (JwwEntity::Dimension(_), Some(source)) => dimension_mode(&source.body, self.version).unwrap_or(0),
            _ => 0,
        };
        self.entity(out, &record.entity, mode);
    }

    fn block(&mut self, out: &mut Writer, block: &JwwBlockRecord) {
        let source = block.source.as_ref().filter(|_| self.reuse);
        self.table.write_object(out, "CDataList", schema(self.version), source.map(|source| source.tag));
        let head = source.filter(|source| {
            let mut reader = Reader::new(&source.head, 0);
            let read = (|| -> Result<_> {
                Ok((
                    read_base(&mut reader, self.version)?,
                    reader.u32()?,
                    reader.u32()? != 0,
                    reader.u32()?,
                    reader.cstring()?,
                ))
            })();
            read.ok() == Some((block.base, block.number, block.referenced, block.time, block.name.clone()))
                && reader.remaining() == 0
        });
        match head {
            Some(source) => out.raw(&source.head),
            None => {
                write_base(out, &block.base, self.version);
                out.u32(block.number);
                out.u32(u32::from(block.referenced));
                out.u32(block.time);
                out.cstring(&block.name, out.unicode);
            }
        }
        self.records(out, &block.entities);
    }

    /// 図形の本体（jwdatafmt.txt の各クラスの Serialize と同じ並び）
    fn entity(&mut self, out: &mut Writer, entity: &JwwEntity, mode: u16) {
        let version = self.version;
        match entity {
            JwwEntity::Line(e) => write_line(out, e, version),
            JwwEntity::Arc(e) => {
                write_base(out, &e.base, version);
                out.f64s(&[e.cx, e.cy, e.radius, e.start, e.sweep, e.tilt, e.flatness]);
                out.u32(u32::from(e.full));
            }
            JwwEntity::Point(e) => write_point(out, e, version),
            JwwEntity::Text(e) => write_text(out, e, version),
            JwwEntity::Solid(e) => {
                // 円のソリッドと分ける線種（101 以上は円のソリッド）
                let base = self.solid_style(e.base, false);
                write_base(out, &base, version);
                // 値の並びは 始点・終点・第 2 点・第 3 点。四角形の順は 始点 → 第 2 点 → 第 3 点 → 終点
                let p = &e.points;
                out.f64s(&[p[0], p[1], p[6], p[7], p[2], p[3], p[4], p[5]]);
                if base.pen_color == 10 {
                    out.u32(e.color.unwrap_or(0));
                }
            }
            JwwEntity::CircleSolid(e) => {
                let base = self.solid_style(e.base, true);
                write_base(out, &base, version);
                out.f64s(&[e.cx, e.cy, e.radius, e.flatness, e.tilt, e.start, e.sweep, e.mode]);
                if base.pen_color == 10 {
                    out.u32(e.color.unwrap_or(0));
                }
            }
            JwwEntity::Block(e) => {
                write_base(out, &e.base, version);
                out.f64s(&[e.x, e.y, e.scale_x, e.scale_y, e.rotation]);
                out.u32(e.definition);
            }
            JwwEntity::Dimension(e) => {
                write_base(out, &e.base, version);
                write_line(out, &e.line, version);
                write_text(out, &e.text, version);
                if version >= 420 {
                    out.u16(mode);
                    // 補助線 1・2、矢印（点）1・2、基準点 1・2。足りなければ、表示しない長さ 0 の線と仮点で補う
                    for index in 0..2 {
                        match e.lines.get(index) {
                            Some(line) => write_line(out, line, version),
                            None => {
                                let (x, y) = if index == 0 { (e.line.x1, e.line.y1) } else { (e.line.x2, e.line.y2) };
                                let line = JwwLine { base: e.line.base, x1: x, y1: y, x2: x, y2: y };
                                write_line(out, &line, version);
                            }
                        }
                    }
                    for index in 0..4 {
                        match e.points.get(index) {
                            Some(point) => write_point(out, point, version),
                            None => {
                                let (x, y) =
                                    if index % 2 == 0 { (e.line.x1, e.line.y1) } else { (e.line.x2, e.line.y2) };
                                let base = JwwBase { pen_style: 1, ..e.line.base };
                                let point = JwwPoint { base, x, y, temporary: true, code: 0, angle: 0.0, scale: 0.0 };
                                write_point(out, &point, version);
                            }
                        }
                    }
                    if e.lines.len() > 2 || e.points.len() > 4 {
                        self.notices.add(
                            NoticeLevel::Warning,
                            "dimension-parts",
                            "寸法の補助線・点は 2 本・4 点までしか書けないため、残りを書いていません",
                        );
                    }
                } else if !e.lines.is_empty() || !e.points.is_empty() {
                    self.notices.add(
                        NoticeLevel::Warning,
                        "dimension-parts",
                        "版 420 より前の JWW は寸法の補助線・点を持たないため、書いていません",
                    );
                }
            }
        }
    }

    /// ソリッドの線種。Jw_cad は線種 101 以上を円のソリッドとして読むので、図形の種類と合わせる
    fn solid_style(&mut self, base: JwwBase, circle: bool) -> JwwBase {
        if circle == (base.pen_style >= 101) {
            return base;
        }
        self.notices.add(
            NoticeLevel::Info,
            "solid-style",
            "ソリッドの線種を図形の種類に合わせました（円のソリッドは 101 以上）",
        );
        JwwBase { pen_style: if circle { 101 } else { 1 }, ..base }
    }
}

/// 図形の共通の値（CData::Serialize）。線幅は版 351 以降
fn write_base(out: &mut Writer, base: &JwwBase, version: u32) {
    out.u32(base.group);
    out.u8(base.pen_style);
    out.u16(base.pen_color);
    if version >= 351 {
        out.u16(base.pen_width);
    }
    out.u16(base.layer);
    out.u16(base.layer_group);
    out.u16(base.flag);
}

fn write_line(out: &mut Writer, line: &JwwLine, version: u32) {
    write_base(out, &line.base, version);
    out.f64s(&[line.x1, line.y1, line.x2, line.y2]);
}

/// 点（CDataTen）。線種 100 の点だけが、点の記号・表示角・表示倍率を持つ
fn write_point(out: &mut Writer, point: &JwwPoint, version: u32) {
    write_base(out, &point.base, version);
    out.f64s(&[point.x, point.y]);
    out.u32(u32::from(point.temporary));
    if point.base.pen_style == 100 {
        out.u32(point.code);
        out.f64s(&[point.angle, point.scale]);
    }
}

fn write_text(out: &mut Writer, text: &JwwText, version: u32) {
    write_base(out, &text.base, version);
    out.f64s(&[text.x1, text.y1, text.x2, text.y2]);
    out.u32(text.text_type);
    out.f64s(&[text.size_x, text.size_y, text.spacing, text.angle]);
    out.cstring(&text.font, out.unicode);
    out.cstring(&text.content, out.unicode);
}

/// 寸法の本体から SXF のモード（版 420 以降）を読む
fn dimension_mode(body: &[u8], version: u32) -> Option<u16> {
    if version < 420 {
        return None;
    }
    let mut reader = Reader::new(body, 0);
    read_base(&mut reader, version).ok()?;
    read_line(&mut reader, version).ok()?;
    read_text(&mut reader, version).ok()?;
    reader.u16().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../tests/fixtures/jww/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// 元のバイト列を使わず、型つきの値だけから書く
    fn rewrite_from_values(bytes: &[u8]) -> Vec<u8> {
        let mut file = JwwFile::read(bytes).unwrap();
        file.source = None;
        for record in &mut file.entities {
            record.source = None;
        }
        for block in &mut file.blocks {
            block.source = None;
            for record in &mut block.entities {
                record.source = None;
            }
        }
        file.write().bytes
    }

    fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
        a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then_some(a.len().min(b.len())))
    }

    #[test]
    fn writes_the_same_bytes_from_the_values_alone() {
        for name in ["minimal-v600.jww", "minimal-v700.jww"] {
            let bytes = fixture(name);
            assert_eq!(first_difference(&rewrite_from_values(&bytes), &bytes), None, "{name}");
        }
    }

    fn position(haystack: &[u8], needle: &[u8]) -> usize {
        haystack.windows(needle.len()).position(|window| window == needle).unwrap()
    }

    /// 違うバイトの範囲（元のバイト列の中）
    fn changed_range(original: &[u8], written: &[u8]) -> std::ops::Range<usize> {
        let prefix = original.iter().zip(written).take_while(|(a, b)| a == b).count();
        let suffix = original.iter().rev().zip(written.iter().rev()).take_while(|(a, b)| a == b).count();
        prefix..original.len().saturating_sub(suffix).max(prefix)
    }

    #[test]
    fn rewrites_only_the_records_that_change() {
        let bytes = fixture("minimal-v700.jww");
        let mut file = JwwFile::read(&bytes).unwrap();
        let body = file.entities[2].source.as_ref().unwrap().body.clone();
        let JwwEntity::Text(text) = &mut file.entities[2].entity else { panic!("text") };
        text.content = "書き換えた文字".into();
        text.x2 = 40.0;
        let written = file.write();
        let start = position(&bytes, &body);
        let changed = changed_range(&bytes, &written.bytes);
        assert!(changed.start >= start && changed.end <= start + body.len(), "{changed:?} / {start}+{}", body.len());
        let read = crate::read_jww(&written.bytes).unwrap();
        let JwwEntity::Text(text) = &read.entities[2] else { panic!("text") };
        assert_eq!((text.content.as_str(), text.x2), ("書き換えた文字", 40.0));
        assert_eq!(read.entities.len(), 6);
    }

    #[test]
    fn numbers_classes_again_after_adding_and_deleting_records() {
        let bytes = fixture("minimal-v600.jww");
        let original = crate::read_jww(&bytes).unwrap();
        let mut file = JwwFile::read(&bytes).unwrap();
        // 最初の線（CDataSen のクラスを登録したレコード）を消し、線と点を足す。後ろの線とブロックの中の線は、
        // クラスを登録し直して読める
        file.entities.remove(0);
        let line = JwwLine {
            base: JwwBase { pen_style: 1, pen_color: 2, ..JwwBase::default() },
            x1: 1.0,
            y1: 2.0,
            x2: 3.0,
            y2: 4.0,
        };
        file.entities.push(JwwRecord::new(JwwEntity::Line(line.clone())));
        let point = JwwPoint {
            base: JwwBase { pen_style: 1, ..JwwBase::default() },
            x: 5.0,
            y: 6.0,
            temporary: false,
            code: 0,
            angle: 0.0,
            scale: 0.0,
        };
        file.entities.push(JwwRecord::new(JwwEntity::Point(point.clone())));
        file.blocks[0].entities.push(JwwRecord::new(JwwEntity::Point(point.clone())));
        let written = file.write();
        assert!(written.notices.is_empty(), "{:?}", written.notices);
        let read = crate::read_jww(&written.bytes).unwrap();
        let mut expected = original.entities[1..].to_vec();
        expected.push(JwwEntity::Line(line));
        expected.push(JwwEntity::Point(point.clone()));
        assert_eq!(read.entities, expected);
        assert_eq!(read.blocks[0].entities[1], JwwEntity::Point(point));
        assert_eq!(read.blocks[0].entities[0], original.blocks[0].entities[0]);
        // 書いたものを読んで書くと同じ
        assert_eq!(JwwFile::read(&written.bytes).unwrap().write().bytes, written.bytes);
    }

    #[test]
    fn writes_the_records_again_in_another_version() {
        let bytes = fixture("minimal-v600.jww");
        let mut file = JwwFile::read(&bytes).unwrap();
        file.version = 700;
        let written = file.write();
        let read = crate::read_jww(&written.bytes).unwrap();
        let original = crate::read_jww(&bytes).unwrap();
        assert_eq!((read.version, &read.entities, &read.blocks), (700, &original.entities, &original.blocks));
        assert_eq!((read.memo.as_str(), &read.groups), (original.memo.as_str(), &original.groups));
        // 版を 700 に変えた図面の文字列は、新しい図面と同じく UTF-16LE で書く
        assert_eq!(&written.bytes[12..15], [0xff, 0xfe, 0xff]);
    }

    /// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-jww --lib -- --ignored`
    #[test]
    #[ignore = "needs drawings in ZUMEN_SAMPLES"]
    fn writes_the_samples_from_the_values_alone() {
        let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
        let mut checked = 0;
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jww")) {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let written = rewrite_from_values(&bytes);
            assert_eq!(first_difference(&written, &bytes), None, "{}", path.display());
            checked += 1;
        }
        assert!(checked > 0);
    }
}
