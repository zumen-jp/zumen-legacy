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
    let body = if digits == 0 { text } else { format!("{}.{}", &text[..split], &text[split..]) };
    if negative { format!("-{body}") } else { body }
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
        for (value, expected) in [(1e21, "1e+21"), (1.5e-7, "1.5e-7"), (123.456, "123.456"), (0.000001, "0.000001")] {
            assert_eq!(number_to_string(value), expected);
        }
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
