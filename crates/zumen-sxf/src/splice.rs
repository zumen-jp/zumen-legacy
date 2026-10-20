//! 元のファイルのバイト列を、位置を指定した置き換え・削除・挿入で書き換える（SFC・P21 のその場の編集）

/// 置き換え（start..end を text に）。start == end なら挿入
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Splice {
    pub start: usize,
    pub end: usize,
    pub text: Vec<u8>,
}

/// 書き換えを集める。同じ位置の挿入は加えた順に並べ、同じ位置から始まる置き換え・削除より前に入れる
#[derive(Debug, Default)]
pub(crate) struct Splices {
    items: Vec<(Splice, usize)>,
}

impl Splices {
    pub(crate) fn replace(&mut self, start: usize, end: usize, text: Vec<u8>) {
        let order = self.items.len();
        self.items.push((Splice { start, end, text }, order));
    }

    pub(crate) fn insert(&mut self, at: usize, text: Vec<u8>) {
        self.replace(at, at, text);
    }

    pub(crate) fn delete(&mut self, start: usize, end: usize) {
        self.replace(start, end, Vec::new());
    }

    /// 書き換えを当てる。重なる置き換えは前のものを優先する（重ならないように集めるので、起きない）
    pub(crate) fn apply(mut self, source: &[u8]) -> Vec<u8> {
        self.items.sort_by_key(|(splice, order)| (splice.start, splice.end > splice.start, *order));
        let mut out = Vec::with_capacity(source.len());
        let mut position = 0;
        for (splice, _) in self.items {
            let start = splice.start.clamp(position, source.len());
            out.extend_from_slice(&source[position..start]);
            out.extend_from_slice(&splice.text);
            position = splice.end.clamp(start, source.len());
        }
        out.extend_from_slice(&source[position..]);
        out
    }
}

/// 位置 at から続く空白（改行を含む）の終わり
pub(crate) fn skip_space(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while bytes.get(end).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')) {
        end += 1;
    }
    end
}

/// ファイルの改行（CR LF か LF）
pub(crate) fn line_ending(bytes: &[u8]) -> &'static [u8] {
    match bytes.iter().position(|byte| *byte == b'\n') {
        Some(at) if at > 0 && bytes[at - 1] == b'\r' => b"\r\n",
        _ => b"\n",
    }
}

/// LF で書いた文字列を、ファイルの改行にする
pub(crate) fn with_line_ending(text: &[u8], ending: &[u8]) -> Vec<u8> {
    if ending == b"\n" {
        return text.to_vec();
    }
    let mut out = Vec::with_capacity(text.len() + text.len() / 40);
    for byte in text {
        if *byte == b'\n' {
            out.extend_from_slice(ending);
        } else {
            out.push(*byte);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_replacements_deletions_and_insertions_in_order() {
        let source = b"a1\nb2\nc3\n";
        let mut splices = Splices::default();
        splices.replace(3, 5, b"B".to_vec());
        splices.insert(3, b"x\n".to_vec());
        splices.insert(3, b"y\n".to_vec());
        splices.delete(6, 9);
        splices.insert(9, b"z\n".to_vec());
        assert_eq!(splices.apply(source), b"a1\nx\ny\nB\nz\n");
        assert_eq!(skip_space(b"ab \r\n\tc", 2), 6);
        assert_eq!(line_ending(b"ISO;\r\nHEADER;"), b"\r\n");
        assert_eq!(with_line_ending(b"a\nb\n", b"\r\n"), b"a\r\nb\r\n");
    }
}
