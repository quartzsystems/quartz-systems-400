// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `printf format [arg...]`: the format is reused until the arguments
//! are consumed.

use super::fail;
use super::io::echo_escapes;
use crate::msg::Msg;
use crate::shell::{Res, Shell};

/// One conversion's flags, width and precision.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: usize,
    prec: Option<usize>,
}

struct State<'a> {
    args: &'a [String],
    next: usize,
    errors: Vec<String>,
}

impl State<'_> {
    fn arg(&mut self) -> Option<&str> {
        let a = self.args.get(self.next).map(String::as_str);
        if a.is_some() {
            self.next += 1;
        }
        a
    }

    fn int(&mut self) -> i64 {
        let Some(a) = self.arg() else { return 0 };
        let a = a.to_string();
        match parse_int(&a) {
            Some(v) => v,
            None => {
                self.errors.push(a.clone());
                parse_prefix(&a)
            }
        }
    }

    fn float(&mut self) -> f64 {
        let Some(a) = self.arg() else { return 0.0 };
        let a = a.to_string();
        if let Some(c) = a.strip_prefix(['\'', '"']) {
            return c.chars().next().map_or(0.0, |c| f64::from(c as u32));
        }
        match a.trim().parse::<f64>() {
            Ok(v) => v,
            Err(_) => match parse_int(&a) {
                #[allow(clippy::cast_precision_loss)]
                Some(v) => v as f64,
                None => {
                    self.errors.push(a.clone());
                    0.0
                }
            },
        }
    }
}

/// An integer operand: decimal, `0` octal, `0x` hex, or `'c` for a
/// character's code.
fn parse_int(s: &str) -> Option<i64> {
    if let Some(c) = s.strip_prefix(['\'', '"']) {
        return Some(c.chars().next().map_or(0, |c| i64::from(c as u32)));
    }
    let t = s.trim_start();
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if t.is_empty() {
        return None;
    }
    let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else if t.len() > 1 && t.starts_with('0') {
        u64::from_str_radix(&t[1..], 8).ok()?
    } else {
        t.parse::<u64>().ok()?
    };
    #[allow(clippy::cast_possible_wrap)]
    let v = v as i64;
    Some(if neg { v.wrapping_neg() } else { v })
}

/// The leading decimal digits of a bad operand.
fn parse_prefix(s: &str) -> i64 {
    let t = s.trim_start();
    let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0)
}

fn pad(s: &str, spec: &Spec, numeric: bool) -> String {
    let len = s.chars().count();
    if len >= spec.width {
        return s.to_string();
    }
    let fill = spec.width - len;
    if spec.left {
        format!("{s}{}", " ".repeat(fill))
    } else if spec.zero && numeric {
        // Zeros go after any sign or base prefix.
        let split = if s.starts_with("0x") || s.starts_with("0X") {
            2
        } else {
            usize::from(s.starts_with(['-', '+', ' ']))
        };
        format!("{}{}{}", &s[..split], "0".repeat(fill), &s[split..])
    } else {
        format!("{}{s}", " ".repeat(fill))
    }
}

fn format_int(v: i64, conv: char, spec: &Spec) -> String {
    #[allow(clippy::cast_sign_loss)]
    let (neg, mag) = match conv {
        'd' | 'i' => (v < 0, v.unsigned_abs()),
        _ => (false, v as u64),
    };
    let mut digits = match conv {
        'o' => format!("{mag:o}"),
        'x' => format!("{mag:x}"),
        'X' => format!("{mag:X}"),
        _ => mag.to_string(),
    };
    if let Some(p) = spec.prec {
        if p == 0 && mag == 0 {
            digits.clear();
        }
        while digits.len() < p {
            digits.insert(0, '0');
        }
    }
    if spec.alt && conv == 'o' && !digits.starts_with('0') {
        digits.insert(0, '0');
    }
    let prefix = if neg {
        "-"
    } else if matches!(conv, 'd' | 'i') && spec.plus {
        "+"
    } else if matches!(conv, 'd' | 'i') && spec.space {
        " "
    } else if spec.alt && mag != 0 && conv == 'x' {
        "0x"
    } else if spec.alt && mag != 0 && conv == 'X' {
        "0X"
    } else {
        ""
    };
    let s = format!("{prefix}{digits}");
    let spec2 = Spec {
        zero: spec.zero && spec.prec.is_none(),
        ..*spec
    };
    pad(&s, &spec2, true)
}

/// C-style `%e`: one digit, a point, `prec` digits, `e`, sign, two or
/// more exponent digits.
fn exp_form(v: f64, prec: usize, upper: bool) -> String {
    let s = format!("{v:.prec$e}");
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = exp.parse().unwrap_or(0);
    let sign = if e < 0 { '-' } else { '+' };
    let r = format!("{mant}e{sign}{:02}", e.abs());
    if upper { r.to_uppercase() } else { r }
}

fn trim_zeros(s: &str) -> String {
    if let Some((m, e)) = s.split_once(['e', 'E']) {
        let sep = if s.contains('E') { 'E' } else { 'e' };
        let m = if m.contains('.') {
            m.trim_end_matches('0').trim_end_matches('.')
        } else {
            m
        };
        return format!("{m}{sep}{e}");
    }
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

fn format_float(v: f64, conv: char, spec: &Spec) -> String {
    let prec = spec.prec.unwrap_or(6);
    let body = if v.is_infinite() {
        if v > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        }
    } else if v.is_nan() {
        "nan".to_string()
    } else {
        match conv {
            'e' | 'E' => exp_form(v, prec, conv == 'E'),
            'g' | 'G' => {
                let p = if prec == 0 { 1 } else { prec };
                let x = if v == 0.0 {
                    0
                } else {
                    exp_form(v, p - 1, false)
                        .split_once('e')
                        .and_then(|(_, e)| e.parse::<i64>().ok())
                        .unwrap_or(0)
                };
                let pi = i64::try_from(p).unwrap_or(6);
                let s = if x < -4 || x >= pi {
                    exp_form(v, p - 1, conv == 'G')
                } else {
                    let decimals = usize::try_from(pi - 1 - x).unwrap_or(0);
                    format!("{v:.decimals$}")
                };
                if spec.alt { s } else { trim_zeros(&s) }
            }
            _ => format!("{v:.prec$}"),
        }
    };
    let s = if v >= 0.0 && spec.plus {
        format!("+{body}")
    } else if v >= 0.0 && spec.space {
        format!(" {body}")
    } else {
        body
    };
    pad(&s, spec, v.is_finite())
}

/// Process a format string escape at `chars[i] == '\\'`; returns the
/// index after it.
fn format_escape(chars: &[char], i: usize, out: &mut String) -> usize {
    let Some(&c) = chars.get(i + 1) else {
        out.push('\\');
        return i + 1;
    };
    let simple = match c {
        'a' => Some('\x07'),
        'b' => Some('\x08'),
        'f' => Some('\x0c'),
        'n' => Some('\n'),
        'r' => Some('\r'),
        't' => Some('\t'),
        'v' => Some('\x0b'),
        '\\' => Some('\\'),
        '"' => Some('"'),
        '\'' => Some('\''),
        _ => None,
    };
    if let Some(s) = simple {
        out.push(s);
        return i + 2;
    }
    if c.is_digit(8) {
        let mut v = 0u32;
        let mut j = i + 1;
        while j < chars.len() && j < i + 4 && chars[j].is_digit(8) {
            v = v * 8 + chars[j].to_digit(8).unwrap_or(0);
            j += 1;
        }
        out.push(char::from_u32(v).unwrap_or('\0'));
        return j;
    }
    out.push('\\');
    out.push(c);
    i + 2
}

/// One pass over the format. Returns false when output must stop
/// (`\c` in `%b`) or the format is bad.
fn one_pass(fmt: &[char], st: &mut State<'_>, out: &mut String) -> Result<bool, String> {
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        if c == '\\' {
            i = format_escape(fmt, i, out);
            continue;
        }
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if fmt.get(i) == Some(&'%') {
            out.push('%');
            i += 1;
            continue;
        }
        let mut spec = Spec::default();
        while let Some(&f) = fmt.get(i) {
            match f {
                '-' => spec.left = true,
                '+' => spec.plus = true,
                ' ' => spec.space = true,
                '#' => spec.alt = true,
                '0' => spec.zero = true,
                _ => break,
            }
            i += 1;
        }
        if fmt.get(i) == Some(&'*') {
            let w = st.int();
            if w < 0 {
                spec.left = true;
            }
            spec.width = usize::try_from(w.unsigned_abs()).unwrap_or(0);
            i += 1;
        } else {
            while let Some(d) = fmt.get(i).and_then(|c| c.to_digit(10)) {
                spec.width = spec.width * 10 + d as usize;
                i += 1;
            }
        }
        if fmt.get(i) == Some(&'.') {
            i += 1;
            if fmt.get(i) == Some(&'*') {
                spec.prec = usize::try_from(st.int()).ok();
                i += 1;
            } else {
                let mut p = 0usize;
                while let Some(d) = fmt.get(i).and_then(|c| c.to_digit(10)) {
                    p = p * 10 + d as usize;
                    i += 1;
                }
                spec.prec = Some(p);
            }
        }
        let Some(&conv) = fmt.get(i) else {
            return Err("the format ends inside a conversion.".to_string());
        };
        i += 1;
        match conv {
            's' => {
                let a = st.arg().unwrap_or("").to_string();
                let a: String = match spec.prec {
                    Some(p) => a.chars().take(p).collect(),
                    None => a,
                };
                out.push_str(&pad(&a, &spec, false));
            }
            'b' => {
                let a = st.arg().unwrap_or("").to_string();
                let mut s = String::new();
                let go = echo_escapes(&a, &mut s);
                let s: String = match spec.prec {
                    Some(p) => s.chars().take(p).collect(),
                    None => s,
                };
                out.push_str(&pad(&s, &spec, false));
                if !go {
                    return Ok(false);
                }
            }
            'c' => {
                let a = st
                    .arg()
                    .unwrap_or("")
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default();
                out.push_str(&pad(&a, &spec, false));
            }
            'd' | 'i' | 'o' | 'u' | 'x' | 'X' => {
                let v = st.int();
                out.push_str(&format_int(v, conv, &spec));
            }
            'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
                let v = st.float();
                out.push_str(&format_float(v, conv, &spec));
            }
            other => return Err(format!("conversion %{other} is not valid.")),
        }
    }
    Ok(true)
}

/// `printf format [arg...]`.
pub(crate) fn printf(sh: &mut Shell, args: &[String]) -> Res {
    let mut a = &args[1..];
    if a.first().is_some_and(|f| f == "--") {
        a = &a[1..];
    }
    let Some(fmt) = a.first() else {
        return Err(fail(
            sh,
            &Msg::Usage("printf".to_string(), "a format is required.".to_string()),
            1,
        ));
    };
    let fmt: Vec<char> = fmt.chars().collect();
    let mut st = State {
        args: &a[1..],
        next: 0,
        errors: Vec::new(),
    };
    let mut out = String::new();
    let mut status = 0;
    loop {
        let before = st.next;
        match one_pass(&fmt, &mut st, &mut out) {
            Ok(true) => {}
            Ok(false) => break,
            Err(d) => {
                sh.out(&out)?;
                return Err(fail(sh, &Msg::Usage("printf".to_string(), d), 1));
            }
        }
        if st.next >= st.args.len() || st.next == before {
            break;
        }
    }
    for e in std::mem::take(&mut st.errors) {
        sh.report(&Msg::NotNumber("printf".to_string(), e));
        status = 1;
    }
    sh.out(&out)?;
    Ok(status)
}
