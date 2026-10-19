//! JavaScript と同じ結果を返す数の扱い。
//!
//! TypeScript の読み取り（packages/）と同じ Scene を返すために、`Math.max`・`Math.hypot`・
//! `Number.prototype.toFixed`・`String(number)`・`Intl.NumberFormat("ja-JP")`・`String.prototype.trim` の
//! 規則をそのまま写す。三角関数は libm（fdlibm 系）を使い、どの環境（WASM を含む）でも同じ値にする。

/// `Math.max(a, b)`。どちらかが NaN なら NaN、+0 は -0 より大きい
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() && b.is_sign_negative() { -0.0 } else { 0.0 };
    }
    if a > b { a } else { b }
}

/// `Math.min(a, b)`。どちらかが NaN なら NaN、-0 は +0 より小さい
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == 0.0 && b == 0.0 {
        return if a.is_sign_negative() || b.is_sign_negative() { -0.0 } else { 0.0 };
    }
    if a < b { a } else { b }
}

/// `Math.hypot(...values)`。V8 と同じく、最大値で割った値の 2 乗を Kahan の加算で足す
pub fn hypot(values: &[f64]) -> f64 {
    let mut max = 0.0f64;
    let mut nan = false;
    for &value in values {
        if value.is_nan() {
            nan = true;
        } else if value.abs() > max {
            max = value.abs();
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for &value in values {
        let n = value.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// `Math.hypot(x, y)`
pub fn hypot2(x: f64, y: f64) -> f64 {
    hypot(&[x, y])
}

#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[inline]
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[inline]
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}

/// ECMAScript の ToInt32（ビット演算の前の変換）。NaN と無限大は 0、整数部を 2^32 で折り返す
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    x.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

/// ECMAScript の ToUint32（`>>>` の前の変換）
pub fn to_uint32(x: f64) -> u32 {
    to_int32(x) as u32
}

/// `Number.parseInt(text, 10)`。先頭の空白を除き、符号と十進の数字が続く間を読む。数字がなければ NaN
pub fn parse_int(text: &str) -> f64 {
    let text = text.trim_start_matches(is_whitespace);
    let (negative, rest) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let digits = &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()];
    if digits.is_empty() {
        return f64::NAN;
    }
    let value = digits.parse::<f64>().unwrap_or(f64::NAN);
    if negative { -value } else { value }
}

/// 0x・0o・0b の後の数字を、その基数の整数として読む（すべてが数字でなければ NaN）
fn radix_integer(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return f64::NAN;
    }
    // 128 ビットに収まれば正確に、収まらなければ（実際の図面にはない）f64 で足していく
    match u128::from_str_radix(digits, radix) {
        Ok(value) => value as f64,
        Err(_) => {
            digits.chars().fold(0.0, |value, c| value * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0)))
        }
    }
}

/// `Number(text)`（StringToNumber）。前後の空白を除き、空なら 0。十進（符号と Infinity を含む）と、
/// 符号のない 0x・0o・0b の整数を読み、それ以外は NaN
pub fn string_to_number(text: &str) -> f64 {
    let text = trim(text);
    if text.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = text.strip_prefix(prefix) {
            return radix_integer(digits, radix);
        }
    }
    let (negative, body) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    // 符号は 1 つだけ（"--1" は NaN。Rust の f64 の読み取りは残りの "-1" を読んでしまう）
    if body.starts_with(['+', '-']) {
        return f64::NAN;
    }
    let value = if body == "Infinity" {
        f64::INFINITY
    } else if body.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-')) {
        // Rust の f64 の読み取りは、この文字の範囲では StrUnsignedDecimalLiteral と同じ書き方だけを受け付ける
        body.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };
    if negative { -value } else { value }
}

/// `x.toPrecision(precision)`。2 進の値を正確に十進展開し、有効数字 precision 桁で、ちょうど半分は大きい方に丸める
pub fn to_precision(x: f64, precision: usize) -> String {
    assert!((1..=100).contains(&precision), "toPrecision の桁は 1 から 100 まで");
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x < 0.0 {
        return format!("-{}", to_precision(-x, precision));
    }
    let (digits, e) = if x == 0.0 {
        ("0".repeat(precision), 0i32)
    } else {
        // 正確な十進展開（小数部の 2 進の桁数だけ十進の桁を取れば丸めは起きない）
        let exponent = ((x.to_bits() >> 52) & 0x7ff) as i64;
        let exact = (1075 - exponent.max(1)).max(0) as usize;
        let expanded = format!("{x:.exact$}");
        let (integer, fraction) = expanded.split_once('.').unwrap_or((&expanded, ""));
        let all: Vec<u8> = integer.bytes().chain(fraction.bytes()).collect();
        let first = all.iter().position(|digit| *digit != b'0').unwrap_or(0);
        // 最初の有効数字の位（10 の何乗か）
        let mut e = integer.len() as i32 - 1 - first as i32;
        let mut kept: Vec<u8> = all[first..].iter().copied().chain(std::iter::repeat(b'0')).take(precision).collect();
        if all.get(first + precision).is_some_and(|digit| *digit >= b'5') {
            increment(&mut kept);
            if kept.len() > precision {
                kept.truncate(precision);
                e += 1;
            }
        }
        (String::from_utf8(kept).expect("ascii digits"), e)
    };
    let p = precision as i32;
    if e < -6 || e >= p {
        let mantissa = if precision == 1 { digits.clone() } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        return format!("{mantissa}e{}{}", if e >= 0 { '+' } else { '-' }, e.abs());
    }
    if e == p - 1 {
        return digits;
    }
    if e >= 0 {
        let split = (e + 1) as usize;
        return format!("{}.{}", &digits[..split], &digits[split..]);
    }
    format!("0.{}{digits}", "0".repeat((-(e + 1)) as usize))
}

/// `Math.round(x)`。ちょうど半分は大きい方（+∞ の側）に丸め、-0.5 以上の負の数は -0 になる
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    if (-0.5..0.0).contains(&x) {
        return -0.0;
    }
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}

/// `Number.parseFloat(text)`。先頭の空白を除き、十進の数（Infinity を含む）として読める最も長い先頭部分を読む。
/// 読めなければ NaN
pub fn parse_float(text: &str) -> f64 {
    let text = text.trim_start_matches(is_whitespace);
    let bytes = text.as_bytes();
    let (negative, start) = match bytes.first() {
        Some(b'-') => (true, 1),
        Some(b'+') => (false, 1),
        _ => (false, 0),
    };
    let signed = |value: f64| if negative { -value } else { value };
    if text[start..].starts_with("Infinity") {
        return signed(f64::INFINITY);
    }
    let digits = |from: usize| bytes[from..].iter().take_while(|byte| byte.is_ascii_digit()).count();
    let integer = digits(start);
    let mut end = start + integer;
    if bytes.get(end) == Some(&b'.') {
        let fraction = digits(end + 1);
        if integer == 0 && fraction == 0 {
            return f64::NAN;
        }
        end += 1 + fraction;
    } else if integer == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent > 0 {
            end += 1 + sign + exponent;
        }
    }
    // Rust の f64 の読み取りも十進から最も近い値に丸める（"5."・".5"・"1e5" の書き方も同じ）
    signed(text[start..end].parse::<f64>().unwrap_or(f64::NAN))
}

/// `String(x)`（Number::toString）。最短の十進表記で、指数は 1e21 以上と 1e-7 未満だけに使う
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x < 0.0 {
        return format!("-{}", number_to_string(-x));
    }
    // Rust の {:e} は最短の桁を返す（例: 1.2345e-7）
    let scientific = format!("{x:e}");
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().expect("exponent") + 1;
    if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let sign = if e >= 0 { '+' } else { '-' };
        if k == 1 {
            format!("{digits}e{sign}{}", e.abs())
        } else {
            format!("{}.{}e{sign}{}", &digits[..1], &digits[1..], e.abs())
        }
    }
}

/// `x.toFixed(digits)`。2 進の値を正確に十進展開し、ちょうど半分は大きい方に丸める
pub fn to_fixed(x: f64, digits: usize) -> String {
    assert!(digits <= 100, "toFixed の桁は 100 まで");
    if x.is_nan() {
        return "NaN".into();
    }
    if x.abs() >= 1e21 {
        return number_to_string(x);
    }
    let negative = x < 0.0;
    let x = x.abs();
    let body = match scaled_integer(x, digits) {
        Some(n) => with_point(n, digits),
        None => fixed_exactly(x, digits),
    };
    if negative { format!("-{body}") } else { body }
}

/// 1e21 未満の正の数 x の toFixed の n（x × 10^digits に最も近い整数、ちょうど半分は大きい方）。
/// 2 進の値をそのまま 128 ビットの整数で計算する。収まらない桁数（18 桁以上）は None
fn scaled_integer(x: f64, digits: usize) -> Option<u128> {
    if digits > 17 {
        return None;
    }
    let bits = x.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i64;
    let mantissa = bits & ((1u64 << 52) - 1);
    let (significand, power) =
        if exponent == 0 { (mantissa, -1074) } else { (mantissa | (1u64 << 52), exponent - 1075) };
    let scale = 10u128.pow(digits as u32);
    if power >= 0 {
        // 2^70 未満の整数 × 10^17 未満なので 128 ビットに収まる
        return Some((u128::from(significand) << power) * scale);
    }
    let product = u128::from(significand) * scale;
    let shift = (-power) as u32;
    if shift >= 128 {
        // 2^-75 未満の数は、10^17 倍しても半分に届かない
        return Some(0);
    }
    let quotient = product >> shift;
    let remainder = product & ((1u128 << shift) - 1);
    Some(if remainder >= 1u128 << (shift - 1) { quotient + 1 } else { quotient })
}

/// 整数 n を、小数点以下 digits 桁の十進の文字列にする（n / 10^digits）
fn with_point(n: u128, digits: usize) -> String {
    let text = n.to_string();
    if digits == 0 {
        return text;
    }
    let padded = if text.len() <= digits { format!("{}{text}", "0".repeat(digits + 1 - text.len())) } else { text };
    let split = padded.len() - digits;
    format!("{}.{}", &padded[..split], &padded[split..])
}

/// 正の数の toFixed。2 進の値を正確に十進展開し、ちょうど半分は大きい方に丸める
fn fixed_exactly(x: f64, digits: usize) -> String {
    // 小数部の 2 進の桁数だけ十進の桁を取れば、展開は正確（丸めが起きない）
    let exponent = ((x.to_bits() >> 52) & 0x7ff) as i64;
    let exact = (1075 - exponent.max(1)).max(0) as usize;
    let expanded = format!("{x:.*}", exact.max(digits + 1));
    let (integer, fraction) = expanded.split_once('.').unwrap_or((&expanded, ""));
    let mut number: Vec<u8> = integer.bytes().chain(fraction.bytes().take(digits)).collect();
    if fraction.as_bytes().get(digits).is_some_and(|digit| *digit >= b'5') {
        increment(&mut number);
    }
    let split = number.len() - digits;
    let text = String::from_utf8(number).expect("ascii digits");
    if digits == 0 { text } else { format!("{}.{}", &text[..split], &text[split..]) }
}

/// 十進の数字の列に 1 を足す（桁上がりで桁が増えることがある）
fn increment(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            return;
        }
    }
    digits.insert(0, b'1');
}

/// `new Intl.NumberFormat("ja-JP", { maximumFractionDigits }).format(value)`。
/// 最短の十進表記を小数 max_fraction 桁で四捨五入し、末尾の 0 を省き、整数部を 3 桁ごとに区切る
pub fn format_ja(value: f64, max_fraction: usize) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "∞" } else { "-∞" }.into();
    }
    // Display は指数を使わない最短の十進表記
    let plain = format!("{}", value.abs());
    let (integer, fraction) = plain.split_once('.').unwrap_or((&plain, ""));
    let mut number: Vec<u8> = integer.bytes().chain(fraction.bytes().take(max_fraction)).collect();
    let kept = fraction.len().min(max_fraction);
    if fraction.as_bytes().get(max_fraction).is_some_and(|digit| *digit >= b'5') {
        increment(&mut number);
    }
    let split = number.len() - kept;
    let text = String::from_utf8(number).expect("ascii digits");
    let (integer, fraction) = text.split_at(split);
    let fraction = fraction.trim_end_matches('0');
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3 + 1);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if fraction.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped}.{fraction}") }
}

/// `new Intl.NumberFormat("ja-JP", { maximumFractionDigits: 3 }).format(Number(value.toFixed(digits)))`。
/// 読み取りの属性の値（座標・寸法）をこの形で示す
pub fn format_number(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return number_to_string(value);
    }
    let fixed = to_fixed(value, digits).parse::<f64>().unwrap_or(f64::NAN);
    format_ja(fixed, 3)
}

/// `String.prototype.trim` が取り除く空白（ECMAScript の WhiteSpace と LineTerminator）
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `String.prototype.trim`
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_whitespace)
}

pub use crate::sort::sort_by;

/// 配列の添字になる名前（`"0"`〜`"4294967294"` の正規の十進表記）なら、その値
pub fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    let value: u64 = key.parse().ok()?;
    (value < u64::from(u32::MAX)).then_some(value as u32)
}

/// オブジェクトのキーの順（`Object.entries`）に並べ直す。配列の添字になる名前を小さい順に先に置き、
/// ほかの名前は加えた順のまま
pub fn order_object_keys<T>(entries: &mut Vec<(String, T)>) {
    let mut indexed: Vec<(u32, (String, T))> = Vec::new();
    let mut named: Vec<(String, T)> = Vec::new();
    for entry in entries.drain(..) {
        match array_index(&entry.0) {
            Some(index) => indexed.push((index, entry)),
            None => named.push(entry),
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    entries.extend(indexed.into_iter().map(|(_, entry)| entry));
    entries.extend(named);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_like_javascript() {
        // Node.js 24 の toFixed・Intl.NumberFormat("ja-JP")・String の結果
        let cases = [
            (-0.0001, "-0"),
            (0.0005, "0.001"),
            (1.0005, "1"),
            (2.0005, "2.001"),
            (1234567.8915, "1,234,567.891"),
            (-1234.5, "-1,234.5"),
            (1e21, "1,000,000,000,000,000,000,000"),
            (1.5e-7, "0"),
            (0.125, "0.125"),
            (0.0625, "0.063"),
        ];
        for (value, expected) in cases {
            assert_eq!(format_number(value, 3), expected, "{value}");
        }
        assert_eq!(to_fixed(-0.0, 3), "0.000");
        assert_eq!(to_fixed(1.45, 1), "1.4");
        assert_eq!(to_fixed(999.9996, 3), "1000.000");
        // ちょうど半分は大きい方へ、2 進では半分に届かない 1.005 は小さい方へ（Node.js 24 の toFixed）
        for (value, digits, expected) in [
            (0.5, 0, "1"),
            (2.5, 0, "3"),
            (-2.5, 0, "-3"),
            (1.25, 1, "1.3"),
            (1.005, 2, "1.00"),
            (5e-324, 3, "0.000"),
            (123_456_789_012_345_680_000.0, 2, "123456789012345683968.00"),
            (0.000_049_999_999_999_999_996, 4, "0.0000"),
        ] {
            assert_eq!(to_fixed(value, digits), expected, "{value}");
        }
        for (value, expected) in [(1e21, "1e+21"), (1.5e-7, "1.5e-7"), (123.456, "123.456"), (0.000001, "0.000001")] {
            assert_eq!(number_to_string(value), expected);
        }
    }

    #[test]
    fn parses_and_rounds_like_javascript() {
        // Node.js 24 の Number.parseFloat・Math.round の結果
        let cases = [
            ("  1.5e3x", 1500.0),
            ("-.5", -0.5),
            ("5.", 5.0),
            ("1e", 1.0),
            ("1e+", 1.0),
            ("\u{3000}12abc", 12.0),
            ("+Infinityx", f64::INFINITY),
            ("0x10", 0.0),
            ("1.2.3", 1.2),
        ];
        for (text, expected) in cases {
            assert_eq!(parse_float(text), expected, "{text}");
        }
        for text in ["", ".", "-", "e5", ".e5", "Inf", "abc"] {
            assert!(parse_float(text).is_nan(), "{text}");
        }
        assert!(parse_float("-0").is_sign_negative());
        for (value, expected) in [(2.5, 3.0), (-2.5, -2.0), (0.49999999999999994, 0.0), (1.5, 2.0), (-1.5, -1.0)] {
            assert_eq!(round(value), expected, "{value}");
        }
        assert!(round(-0.3).is_sign_negative());
        assert!(round(f64::NAN).is_nan());
    }

    #[test]
    fn converts_strings_and_integers_like_javascript() {
        // Node.js 24 の Number()・parseInt・ToInt32・toPrecision の結果
        for (text, expected) in [
            ("", 0.0),
            ("  ", 0.0),
            (" 1.5e3 ", 1500.0),
            ("0x1A", 26.0),
            ("0b101", 5.0),
            ("0o17", 15.0),
            ("-Infinity", f64::NEG_INFINITY),
            (".5", 0.5),
            ("5.", 5.0),
        ] {
            assert_eq!(string_to_number(text), expected, "{text:?}");
        }
        for text in ["-0x10", "1_000", "1e", "abc", "Infinityx", "0x", "1 2", "inf", "nan", "--1", "+-1", "-+Infinity"]
        {
            assert!(string_to_number(text).is_nan(), "{text:?}");
        }
        for (text, expected) in [(" 42abc", 42.0), ("-7", -7.0), ("+3.9", 3.0), ("99999999999999999999", 1e20)] {
            assert_eq!(parse_int(text), expected, "{text:?}");
        }
        assert!(parse_int("x1").is_nan());
        assert!(parse_int("--1").is_nan());
        assert_eq!(to_int32(4_294_967_296.0 + 5.0), 5);
        assert_eq!(to_int32(-1.5), -1);
        assert_eq!(to_int32(3_221_225_472.0), -1_073_741_824);
        assert_eq!(to_uint32(-1.0), 4_294_967_295);
        for (value, expected) in [
            (100.0, "100.000"),
            (0.000123456789, "0.000123457"),
            (1234567.0, "1.23457e+6"),
            (0.0000001, "1.00000e-7"),
            (9.999995, "10.0000"),
            (99999.95, "99999.9"),
            (0.5, "0.500000"),
            (2.5e-7, "2.50000e-7"),
        ] {
            assert_eq!(to_precision(value, 6), expected, "{value}");
        }
        assert_eq!(to_precision(0.0, 3), "0.00");
        assert_eq!(to_precision(-1.25, 2), "-1.3");
    }

    #[test]
    fn hypot_and_max_follow_v8() {
        assert_eq!(hypot2(3.0, 4.0), 5.0);
        assert!(hypot(&[f64::NAN, f64::INFINITY]).is_infinite());
        assert!(max(f64::NAN, 1.0).is_nan());
        assert!(max(-0.0, 0.0).is_sign_positive());
        assert!(min(0.0, -0.0).is_sign_negative());
    }
}
