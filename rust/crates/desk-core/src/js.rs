//! The bits of JavaScript semantics the TS reader relied on without saying so.
//!
//! The golden test is "Rust serializes the same JSON as `JSON.stringify(loadDay())`",
//! and the TS code leans on JS behaviour all over: `String(x)`, truthiness,
//! `trim()` (which strips U+FEFF, unlike `str::trim`), `Math.round` (half up,
//! not half away from zero), `toFixed`, `localeCompare`, the way `JSON.parse`
//! turns every number into a double and `JSON.stringify` prints `1.0` as `1`,
//! and the key order of an object with integer-like keys. Each of them lives
//! here once, so the ports read like the TS.

use serde::Serializer;
use serde_json::{Map, Number, Value};
use std::cmp::Ordering;

/// JS `WhiteSpace` + `LineTerminator`: what `trim()` and `\s` match.
/// Differs from `char::is_whitespace`: U+FEFF is in, U+0085 is out.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `String.prototype.trimEnd`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `s.replace(/\s+/g, ' ')`.
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_space = false;
    for c in s.chars() {
        if is_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// `s.length`: JS counts UTF-16 code units.
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `s.slice(0, n)` with `n` in UTF-16 code units. A character that would be
/// cut in half (an astral one, n landing between its surrogates) is dropped:
/// JS would keep a lone surrogate, which a Rust `String` cannot hold.
pub fn slice16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n {
            return &s[..i];
        }
    }
    s
}

/// `s.slice(n)` with `n` in UTF-16 code units (same caveat as [`slice16`]).
pub fn slice16_from(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        if units >= n {
            return &s[i..];
        }
        units += c.len_utf16();
    }
    ""
}

/// JS truthiness of a parsed JSON value (`undefined` is the caller's `None`).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Truthiness of a property that may be absent (`undefined`).
pub fn truthy_opt(v: Option<&Value>) -> bool {
    v.is_some_and(truthy)
}

/// `x ?? fallback` for a property: absent or `null` is nullish.
pub fn nullish(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// `Math.round`: half rounds towards +∞ (`-2.5 → -2`), unlike `f64::round`.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    // x − floor(x) is exact for every finite double, so the tie test is exact too.
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    // Math.round keeps the sign of a negative input that rounds to zero.
    if r == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        r
    }
}

/// `Math.round(x * 10) / 10`, the one-decimal rounding `normalizeScale` uses.
pub fn round1(x: f64) -> f64 {
    round(x * 10.0) / 10.0
}

/// `Number.prototype.toString()` (radix 10): shortest round-trip digits,
/// decimal notation for 1e-7 < |x| < 1e21, exponent notation outside.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let neg = x < 0.0;
    // `{:e}` is Rust's shortest round-trip form: "1.2345e3", "5e-7".
    let sci = format!("{:e}", x.abs());
    let (mant, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let e: i32 = exp.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = e + 1; // position of the decimal point, as in ECMA-262 Number::toString
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if k <= n && n <= 21 {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-n) as usize));
        out.push_str(&digits);
    } else {
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if n > 0 { '+' } else { '-' });
        out.push_str(&(n - 1).abs().to_string());
    }
    out
}

/// `String(x)` for a parsed JSON value.
pub fn to_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_string(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        // Array.prototype.toString = join(','), null/undefined elements empty.
        Value::Array(a) => a
            .iter()
            .map(|x| {
                if x.is_null() {
                    String::new()
                } else {
                    to_string(x)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// `Number.prototype.toFixed(digits)`: round half up on the exact binary value.
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return number_to_string(x);
    }
    // JS: x < 0 → "-" + toFixed(−x); −0 is not < 0, so it prints without a sign.
    let neg = x < 0.0;
    let a = x.abs();
    // Rust prints the exact expansion up to the precision asked for; 25 more
    // digits than needed settle every tie that a double can actually hold.
    let long = format!("{:.*}", digits + 25, a);
    let (int_part, frac) = long.split_once('.').unwrap_or((long.as_str(), ""));
    let mut ds: Vec<u8> = int_part.bytes().chain(frac.bytes().take(digits)).collect();
    let round_up = frac.as_bytes().get(digits).is_some_and(|d| *d >= b'5');
    if round_up {
        let mut i = ds.len();
        loop {
            if i == 0 {
                ds.insert(0, b'1');
                break;
            }
            i -= 1;
            if ds[i] == b'9' {
                ds[i] = b'0';
            } else {
                ds[i] += 1;
                break;
            }
        }
    }
    let int_len = ds.len() - digits;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&ds[..int_len]).unwrap_or("0"));
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&ds[int_len..]).unwrap_or(""));
    }
    out
}

/// A double as `JSON.stringify` would print it, as a `serde_json` value:
/// integral values become integers (`25.0` → `25`, `-0` → `0`), so the
/// serialized text and the parsed `Value` both match the TS output.
pub fn number(f: f64) -> Value {
    if !f.is_finite() {
        // JSON.stringify(NaN | Infinity) === 'null'
        return Value::Null;
    }
    if f.fract() == 0.0 && f.abs() < 1e21 {
        if f.abs() <= 9_007_199_254_740_992.0 {
            // Exact: |f| ≤ 2^53 fits i64 without loss.
            return Value::Number(Number::from(f as i64));
        }
        // Beyond 2^53 JS prints the shortest digits padded with zeros
        // (2^63 → 9223372036854776000), not the exact value.
        let s = number_to_string(f);
        if let Ok(u) = s.parse::<u64>() {
            return Value::Number(Number::from(u));
        }
        if let Ok(i) = s.parse::<i64>() {
            return Value::Number(Number::from(i));
        }
    }
    Number::from_f64(f).map_or(Value::Null, Value::Number)
}

/// Serde helper: an `f64` field printed the JS way.
pub fn ser_f64<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&number(*x), s)
}

/// Serde helper: an `Option<f64>` field printed the JS way (`None` → `null`).
pub fn ser_opt_f64<S: Serializer>(x: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match x {
        Some(x) => ser_f64(x, s),
        None => s.serialize_none(),
    }
}

/// Is `k` an array index in the sense of ECMA-262 (`"0"`, `"17"`, not `"01"`)?
fn is_index_key(k: &str) -> bool {
    if k == "0" {
        return true;
    }
    let b = k.as_bytes();
    !b.is_empty()
        && b.len() <= 10
        && b[0] != b'0'
        && b.iter().all(u8::is_ascii_digit)
        && k.parse::<u64>().is_ok_and(|n| n < u64::from(u32::MAX))
}

/// Put the keys of an object in the order a JS engine enumerates them:
/// integer-like keys first, ascending, then the rest in insertion order.
pub fn order_keys(m: &mut Map<String, Value>) {
    if !m.keys().any(|k| is_index_key(k)) {
        return;
    }
    let old = std::mem::take(m);
    let (mut idx, rest): (Vec<_>, Vec<_>) = old.into_iter().partition(|(k, _)| is_index_key(k));
    idx.sort_by_key(|(k, _)| k.parse::<u64>().unwrap_or(0));
    m.extend(idx);
    m.extend(rest);
}

/// Make a value parsed by `serde_json` look like one that went through
/// `JSON.parse` + `JSON.stringify`: every number is a double printed the JS
/// way, and object keys are in JS enumeration order.
pub fn jsify(v: &mut Value) {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            // Small integers are already what JS prints; skip the round trip.
            let small = n
                .as_i64()
                .is_some_and(|i| i.unsigned_abs() <= 9_007_199_254_740_992);
            if !small {
                *v = number(f);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(jsify),
        Value::Object(m) => {
            order_keys(m);
            m.values_mut().for_each(jsify);
        }
        _ => {}
    }
}

/// Parse a state file the way `readJson` in macro-state.ts does: UTF-8 with
/// replacement characters for bad bytes, one leading BOM dropped, then
/// `JSON.parse`. `None` when it does not parse (a half-written file).
///
/// Known gaps against `JSON.parse`: nesting deeper than 128 levels, numbers
/// beyond ±1.8e308 (JS: `Infinity`) and lone surrogate escapes (`"\ud800"`)
/// are refused here and read as "no file".
pub fn parse_json(bytes: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let mut v: Value = serde_json::from_str(text).ok()?;
    jsify(&mut v);
    Some(v)
}

/// `Date.parse(`${s}T00:00:00Z`) / 86_400_000` for the date-only ISO forms V8
/// accepts (`YYYY`, `YYYY-MM`, `YYYY-MM-DD`, ±`YYYYYY` variants): days since
/// 1970-01-01, or `None` where JS gets `NaN`. Like V8, a day 29–31 that the
/// month does not have rolls over into the next month (`2026-02-30` = 2 March).
pub fn iso_day_number(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let (sign, year_len, rest) = match b.first() {
        Some(b'+') => (1i64, 6usize, &b[1..]),
        Some(b'-') => (-1i64, 6usize, &b[1..]),
        _ => (1i64, 4usize, b),
    };
    let digits = |x: &[u8]| -> Option<i64> {
        if x.is_empty() || !x.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(x).ok()?.parse().ok()
    };
    if rest.len() < year_len {
        return None;
    }
    let year = sign * digits(&rest[..year_len])?;
    if sign < 0 && year == 0 {
        return None; // "-000000" is not a valid year in ECMA-262
    }
    let rest = &rest[year_len..];
    let (month, day) = match rest.len() {
        0 => (1, 1),
        3 if rest[0] == b'-' => (digits(&rest[1..3])?, 1),
        6 if rest[0] == b'-' && rest[3] == b'-' => (digits(&rest[1..3])?, digits(&rest[4..6])?),
        _ => return None,
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, 1) + day - 1)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The ICU root collation weight class of a character, as V8's
/// `localeCompare` orders plain ASCII: spaces and punctuation (in ICU's own
/// order), then digits, then letters with case ignored. Everything else
/// sorts after, by lower-cased code point — a simplification of the full
/// Unicode collation, which matters only for non-ASCII ids.
fn primary(c: char) -> (u32, u32) {
    const PUNCT: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    if let Some(i) = PUNCT.find(c) {
        return (0, i as u32);
    }
    if c.is_ascii_digit() {
        return (1, c as u32);
    }
    if c.is_ascii_alphabetic() {
        return (2, c.to_ascii_lowercase() as u32);
    }
    let lower = c.to_lowercase().next().unwrap_or(c);
    (3, lower as u32)
}

/// `a.localeCompare(b)` as V8 with full ICU answers it for ids and dates:
/// primary weights first, then lower case before upper case at the first
/// difference, then code points.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let pa = a.chars().map(primary);
    let pb = b.chars().map(primary);
    pa.cmp(pb)
        .then_with(|| {
            let ta = a.chars().map(|c| c.is_uppercase());
            let tb = b.chars().map(|c| c.is_uppercase());
            ta.cmp(tb)
        })
        .then_with(|| a.cmp(b))
}

/// `typeof x === 'string' ? x : x == null ? '' : String(x)` — the `str()` of macro.ts.
pub fn str_of(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) => to_string(v),
    }
}

/// `typeof x === 'string'`.
pub fn as_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

/// `x && typeof x === 'object' && !Array.isArray(x)` — the `rec()` of macro.ts.
pub fn rec(v: Option<&Value>) -> Option<&Map<String, Value>> {
    v.and_then(Value::as_object)
}

/// `typeof x === 'number' && Number.isFinite(x) ? x : null`.
pub fn num_or_null(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|f| f.is_finite())
}

/// The `num()` of macro.ts: a finite number, else 0.
pub fn num(v: Option<&Value>) -> f64 {
    num_or_null(v).unwrap_or(0.0)
}

/// The `intOf()` of macro.ts: `Math.max(0, Math.round(x))`, else 0.
/// Saturates at `u64::MAX` where JS would keep a huge double.
pub fn int_of(v: Option<&Value>) -> u64 {
    match num_or_null(v) {
        Some(f) => {
            let r = round(f);
            if r <= 0.0 {
                0
            } else if r >= u64::MAX as f64 {
                u64::MAX
            } else {
                r as u64
            }
        }
        None => 0,
    }
}

/// The `strs()` of macro.ts: the non-blank strings of an array, trimmed.
pub fn strs(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .map(trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// A property of a value that JS would read without throwing: objects have
/// properties, everything else (`5`, `"x"`, `[]`) reads as `undefined`.
/// (`null.x` throws in JS; the ports read it as `undefined` instead.)
pub fn prop<'a>(v: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    v.and_then(Value::as_object).and_then(|m| m.get(key))
}

/// `{...v}` for a value spread into an object literal: an object's own keys,
/// an array's indices; numbers, booleans and null spread to nothing.
/// (A string spreads to its characters in JS; here it spreads to nothing.)
pub fn spread(v: &Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m.clone(),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .map(|(i, x)| (i.to_string(), x.clone()))
            .collect(),
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trim_follows_js_not_unicode() {
        assert_eq!(trim("\u{FEFF} x \u{3000}"), "x");
        assert_eq!(trim("\u{0085}x"), "\u{0085}x");
        assert_eq!(collapse_spaces("  a \n\t b "), " a b ");
    }

    #[test]
    fn math_round_rounds_half_up() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.499_999_999_999_999_94), 0.0);
        assert!(round(-0.4).is_sign_negative());
        assert_eq!(round1(12.649_999), 12.6);
    }

    #[test]
    fn numbers_print_like_js() {
        for (x, s) in [
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-7"),
            (123_456_789_012_345_680_000.0, "123456789012345680000"),
            (0.000_001, "0.000001"),
            (-0.0, "0"),
            (12.6, "12.6"),
            (-58.0, "-58"),
            (100.0, "100"),
            (1e-7, "1e-7"),
            (2.5e-5, "0.000025"),
            (f64::NAN, "NaN"),
        ] {
            assert_eq!(number_to_string(x), s, "{x}");
        }
        assert_eq!(to_string(&json!([1, [2, null]])), "1,2,");
        assert_eq!(to_string(&json!({})), "[object Object]");
        assert_eq!(to_string(&json!(true)), "true");
    }

    #[test]
    fn to_fixed_matches_v8() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(-0.125, 2), "-0.13");
        assert_eq!(to_fixed(-0.0, 2), "0.00");
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(9.999, 2), "10.00");
        assert_eq!(to_fixed(1.234, 2), "1.23");
        assert_eq!(to_fixed(1e21, 2), "1e+21");
        assert_eq!(to_fixed(18.0, 2), "18.00");
    }

    #[test]
    fn json_numbers_become_what_stringify_prints() {
        let mut v: Value = serde_json::from_str(
            r#"{"a":1.0,"b":-0.0,"c":0.5,"d":9223372036854775808,"e":9007199254740993,"f":1e300}"#,
        )
        .unwrap();
        jsify(&mut v);
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"a":1,"b":0,"c":0.5,"d":9223372036854776000,"e":9007199254740992,"f":1e+300}"#
        );
        assert_eq!(v["a"], json!(1));
    }

    #[test]
    fn integer_keys_come_first_like_in_js() {
        let mut v: Value = serde_json::from_str(r#"{"b":1,"2":2,"1":3,"01":4}"#).unwrap();
        jsify(&mut v);
        assert_eq!(
            serde_json::to_string(&v).unwrap(),
            r#"{"1":3,"2":2,"b":1,"01":4}"#
        );
    }

    #[test]
    fn parse_json_tolerates_a_bom_and_refuses_garbage() {
        assert_eq!(
            parse_json("\u{FEFF}{\"a\":1}".as_bytes()),
            Some(json!({"a": 1}))
        );
        assert_eq!(parse_json(b"{not json"), None);
        assert_eq!(parse_json(b""), None);
        assert_eq!(parse_json(b"{\"ok\": tru"), None);
        assert_eq!(parse_json(b"null"), Some(Value::Null));
        // Bad UTF-8 becomes U+FFFD, as Node's decoder does, instead of failing.
        assert_eq!(parse_json(b"\"a\xffb\""), Some(json!("a\u{FFFD}b")));
    }

    #[test]
    fn iso_dates_parse_like_v8() {
        assert_eq!(iso_day_number("1970-01-01"), Some(0));
        assert_eq!(iso_day_number("2026-02-30"), iso_day_number("2026-03-02"));
        assert_eq!(iso_day_number("2026-04-31"), iso_day_number("2026-05-01"));
        assert_eq!(iso_day_number("2026-01-32"), None);
        assert_eq!(iso_day_number("2026-13-01"), None);
        assert_eq!(iso_day_number("2026-00-10"), None);
        assert_eq!(iso_day_number("2026-1-07"), None);
        assert_eq!(iso_day_number(" 2026-10-07"), None);
        assert_eq!(iso_day_number("2026"), iso_day_number("2026-01-01"));
        assert_eq!(iso_day_number("2026-10"), iso_day_number("2026-10-01"));
        assert_eq!(
            iso_day_number("+002026-10-07"),
            iso_day_number("2026-10-07")
        );
        // Date.parse('0000-01-01T00:00:00Z') = -62167219200000
        assert_eq!(
            iso_day_number("0000-01-01"),
            Some(-62_167_219_200_000 / 86_400_000)
        );
        assert_eq!(iso_day_number("-000000-01-01"), None);
    }

    #[test]
    fn locale_compare_matches_icu_for_ascii() {
        let mut v = vec![
            "bias_a", "bias2", "Bias", "bias", "b-c", "b_c", "a1", "A1", "я", "Я", "z",
        ];
        v.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(
            v,
            ["a1", "A1", "b_c", "b-c", "bias", "Bias", "bias_a", "bias2", "z", "я", "Я"]
        );
        let mut w = vec!["a", "A", "ab", "Ab", "aB", "a b", "ab_", "a_b"];
        w.sort_by(|a, b| locale_compare(a, b));
        assert_eq!(w, ["a", "A", "a b", "a_b", "ab", "aB", "Ab", "ab_"]);
    }

    #[test]
    fn utf16_slicing() {
        assert_eq!(utf16_len("ab😀"), 4);
        assert_eq!(slice16("ab😀c", 3), "ab");
        assert_eq!(slice16("ab😀c", 4), "ab😀");
        assert_eq!(slice16_from("M15 — x", 3), " — x");
        assert_eq!(slice16_from("ab", 5), "");
    }
}
