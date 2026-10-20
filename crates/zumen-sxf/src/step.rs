//! ISO 10303-21（STEP の物理ファイル）のインスタンスを組み立てて書く。P21 の書き出し（p21_writer.rs）と、
//! 元の P21 を書き換える編集（p21_edit.rs）が使う。
//!
//! - 実数は読み直すと同じ値になる最短の桁で、小数点を必ず付ける（`1.`・`0.25`・`1.5E-7`。ISO 10303-21 の REAL）。
//! - 文字列は ASCII の印字できる文字をそのまま（`'` は `''`、`\` は `\\`）、それ以外を `\X2\…\X0\`（UTF-16）か、
//!   基本多言語面の外の文字を含めば `\X4\…\X0\`（UCS-4）で書く。制御文字は書けないので除く。
//! - 複合インスタンスの部分は名前の順に並べる（ISO 10303-21 の外部写像）。

use crate::p21_reader::P21Value;
use std::fmt::Write;

/// インスタンスの引数の値
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum V {
    /// `$`（値なし）
    Unset,
    /// `*`（上位の型で決まる値）
    Derived,
    Int(i64),
    Real(f64),
    Str(String),
    Ref(u64),
    /// `.NAME.`
    Enum(&'static str),
    /// `NAME(value)`
    Typed(&'static str, Box<V>),
    List(Vec<V>),
}

impl V {
    pub(crate) fn str(value: impl Into<String>) -> V {
        V::Str(value.into())
    }

    pub(crate) fn refs(ids: &[u64]) -> V {
        V::List(ids.iter().map(|id| V::Ref(*id)).collect())
    }

    pub(crate) fn typed(name: &'static str, value: V) -> V {
        V::Typed(name, Box::new(value))
    }

    pub(crate) fn logical(value: bool) -> V {
        V::Enum(if value { "T" } else { "F" })
    }
}

/// 型の名前と引数（単純インスタンス、または複合インスタンスの部分）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Part {
    pub name: &'static str,
    pub args: Vec<V>,
}

pub(crate) fn part(name: &'static str, args: Vec<V>) -> Part {
    Part { name, args }
}

/// インスタンスの本体
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Simple(Part),
    Complex(Vec<Part>),
}

/// インスタンスを書く。数にできない実数（NaN・無限大）は 0 にして、その数を数える
#[derive(Default)]
pub(crate) struct StepWriter {
    pub nonfinite: u64,
}

impl StepWriter {
    pub(crate) fn real(&mut self, value: f64, out: &mut String) {
        if !value.is_finite() {
            self.nonfinite += 1;
            out.push_str("0.");
            return;
        }
        out.push_str(&real(value));
    }

    pub(crate) fn value(&mut self, value: &V, out: &mut String) {
        match value {
            V::Unset => out.push('$'),
            V::Derived => out.push('*'),
            V::Int(value) => {
                let _ = write!(out, "{value}");
            }
            V::Real(value) => self.real(*value, out),
            V::Str(text) => string(text, out),
            V::Ref(id) => {
                let _ = write!(out, "#{id}");
            }
            V::Enum(name) => {
                let _ = write!(out, ".{name}.");
            }
            V::Typed(name, inner) => {
                out.push_str(name);
                out.push('(');
                self.value(inner, out);
                out.push(')');
            }
            V::List(items) => self.list(items, out),
        }
    }

    fn list(&mut self, items: &[V], out: &mut String) {
        out.push('(');
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            self.value(item, out);
        }
        out.push(')');
    }

    fn part(&mut self, part: &Part, out: &mut String) {
        out.push_str(part.name);
        self.list(&part.args, out);
    }

    /// `#id=…;`（改行は含めない）
    pub(crate) fn instance(&mut self, id: u64, body: &Body) -> String {
        let mut out = format!("#{id}=");
        match body {
            Body::Simple(part) => self.part(part, &mut out),
            Body::Complex(parts) => {
                let mut sorted: Vec<&Part> = parts.iter().collect();
                sorted.sort_by_key(|part| part.name);
                out.push('(');
                for (index, part) in sorted.into_iter().enumerate() {
                    if index > 0 {
                        out.push(' ');
                    }
                    self.part(part, &mut out);
                }
                out.push(')');
            }
        }
        out.push(';');
        out
    }
}

impl StepWriter {
    /// 読んだインスタンスの値を書く（元のインスタンスを書き換えるとき）。`$` と `*` は区別せず `$` にする
    pub(crate) fn read_value(&mut self, value: &P21Value, out: &mut String) {
        match value {
            P21Value::Null => out.push('$'),
            P21Value::Number(value) => self.real(*value, out),
            P21Value::Text(text) => string(text, out),
            P21Value::Ref(id) => {
                let _ = write!(out, "#{id}");
            }
            P21Value::Enum(name) => {
                let _ = write!(out, ".{name}.");
            }
            P21Value::Typed(name, inner) => {
                out.push_str(name);
                out.push('(');
                // 読み取りは引数が 1 つでない型付きの値を集合にしている
                match inner.as_ref() {
                    P21Value::List(items) => self.read_items(items, out),
                    inner => self.read_value(inner, out),
                }
                out.push(')');
            }
            P21Value::List(items) => {
                out.push('(');
                self.read_items(items, out);
                out.push(')');
            }
        }
    }

    fn read_items(&mut self, items: &[P21Value], out: &mut String) {
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            self.read_value(item, out);
        }
    }

    /// 読んだインスタンス（型ごとの引数）を `#id=…;` にする
    pub(crate) fn read_instance(&mut self, id: u64, parts: &[(String, Vec<P21Value>)]) -> String {
        let mut out = format!("#{id}=");
        let part = |writer: &mut Self, (name, args): &(String, Vec<P21Value>), out: &mut String| {
            out.push_str(name);
            out.push('(');
            writer.read_items(args, out);
            out.push(')');
        };
        if let [single] = parts {
            part(self, single, &mut out);
        } else {
            out.push('(');
            for (index, value) in parts.iter().enumerate() {
                if index > 0 {
                    out.push(' ');
                }
                part(self, value, &mut out);
            }
            out.push(')');
        }
        out.push(';');
        out
    }
}

/// 実数を ISO 10303-21 の REAL にする（有限の値）。Rust の `{:?}` は読み直すと同じ値になる最短の桁で、
/// 1e16 以上・1e-4 未満は指数で書くので、仮数に小数点を補い、指数の印を E にする
pub(crate) fn real(value: f64) -> String {
    if value == 0.0 {
        return "0.".into();
    }
    let text = format!("{value:?}");
    match text.split_once('e') {
        Some((mantissa, exponent)) if mantissa.contains('.') => format!("{mantissa}E{exponent}"),
        Some((mantissa, exponent)) => format!("{mantissa}.E{exponent}"),
        None => text,
    }
}

/// 文字列を `'…'` にする（ISO 10303-21 の 6.4.3）
pub(crate) fn string(text: &str, out: &mut String) {
    out.push('\'');
    let mut run: Vec<char> = Vec::new();
    for c in text.chars() {
        if c.is_control() {
            continue;
        }
        if (' '..='~').contains(&c) {
            flush(&mut run, out);
            match c {
                '\'' => out.push_str("''"),
                '\\' => out.push_str("\\\\"),
                _ => out.push(c),
            }
        } else {
            run.push(c);
        }
    }
    flush(&mut run, out);
    out.push('\'');
}

/// ASCII 以外の文字の並びを \X2\…\X0\（UTF-16 の 4 桁ずつ）か、基本多言語面の外の文字を含めば \X4\…\X0\
/// （符号位置の 8 桁ずつ）にする
fn flush(run: &mut Vec<char>, out: &mut String) {
    if run.is_empty() {
        return;
    }
    if run.iter().all(|c| (*c as u32) <= 0xffff) {
        out.push_str("\\X2\\");
        for c in run.iter() {
            let _ = write!(out, "{:04X}", *c as u32);
        }
    } else {
        out.push_str("\\X4\\");
        for c in run.iter() {
            let _ = write!(out, "{:08X}", *c as u32);
        }
    }
    out.push_str("\\X0\\");
    run.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p21_reader::decode_step_string;

    #[test]
    fn writes_reals_with_a_decimal_point() {
        for (value, text) in [
            (0.0, "0."),
            (-0.0, "0."),
            (1.0, "1.0"),
            (0.25, "0.25"),
            (4453.694643, "4453.694643"),
            (1e16, "1.E16"),
            (-1.5e-7, "-1.5E-7"),
            (1.2345678901234568e17, "1.2345678901234568E17"),
        ] {
            assert_eq!(real(value), text);
            assert_eq!(text.parse::<f64>().unwrap(), value);
        }
        let mut writer = StepWriter::default();
        let mut out = String::new();
        writer.real(f64::NAN, &mut out);
        assert_eq!((out.as_str(), writer.nonfinite), ("0.", 1));
    }

    #[test]
    fn escapes_strings_the_reader_decodes() {
        for text in ["", " ", "it's", "a\\b", "表示ソ能", "ＭＳ ゴシック", "R15 φ30 120°", "𠮷野家", "改\n行"]
        {
            let mut out = String::new();
            string(text, &mut out);
            assert!(out.is_ascii(), "{out}");
            let inner = &out.as_bytes()[1..out.len() - 1];
            assert_eq!(decode_step_string(inner).unwrap(), text.replace('\n', ""), "{out}");
        }
        let mut out = String::new();
        string("ＭＳ ゴシック", &mut out);
        assert_eq!(out, "'\\X2\\FF2DFF33\\X0\\ \\X2\\30B430B730C330AF\\X0\\'");
    }

    #[test]
    fn sorts_the_parts_of_complex_instances() {
        let body = Body::Complex(vec![
            part("STYLED_ITEM", vec![V::refs(&[30]), V::Ref(45)]),
            part("REPRESENTATION_ITEM", vec![V::str(" ")]),
            part("ANNOTATION_OCCURRENCE", vec![]),
        ]);
        assert_eq!(
            StepWriter::default().instance(46, &body),
            "#46=(ANNOTATION_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#30),#45));"
        );
        let simple = Body::Simple(part(
            "TRIMMED_CURVE",
            vec![
                V::str(" "),
                V::Ref(44),
                V::List(vec![V::typed("PARAMETER_VALUE", V::Real(0.5))]),
                V::List(vec![V::Ref(41)]),
                V::logical(true),
                V::Enum("PARAMETER"),
            ],
        ));
        assert_eq!(
            StepWriter::default().instance(45, &simple),
            "#45=TRIMMED_CURVE(' ',#44,(PARAMETER_VALUE(0.5)),(#41),.T.,.PARAMETER.);"
        );
    }
}
