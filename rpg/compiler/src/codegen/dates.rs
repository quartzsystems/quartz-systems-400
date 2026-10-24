// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dates, times and timestamps. Their storage is character data in the
//! field's format (`char(n)` in the IR); literals, figurative constants
//! and defaults are rendered here, everything else calls the runtime
//! library's `_QRNX_DT...` procedures with each operand's address,
//! descriptor and format word (`dtfmt`). A duration (`%DAYS(n)`) is an
//! `i64` register of type `Dur`.

use rpgfront::ast::{BinOp, Expr, Fig, Lit, Op};

use crate::codegen::Gen;
use crate::codegen::emit::quote;
use crate::codegen::expr::{V, Vk, Want};
use crate::diag::ids;
use crate::dtfmt::{self, Class, Conv};
use crate::types::{DatFmt, TimFmt, Ty};

/// Duration units as the runtime numbers them.
const UNITS: &[(&[&str], u8)] = &[
    (&["*YEARS", "*Y"], 0),
    (&["*MONTHS", "*M"], 1),
    (&["*DAYS", "*D"], 2),
    (&["*HOURS", "*H"], 3),
    (&["*MINUTES", "*MN"], 4),
    (&["*SECONDS", "*S"], 5),
    (&["*MSECONDS", "*MS"], 6),
];

/// A format argument the built-in does not take.
const BAD_FORMAT: &str = "RNF0599";
/// A literal too short for a date in its format.
const SHORT_DATE: &str = "RNF7510";
/// A literal too short for a timestamp.
const SHORT_STAMP: &str = "RNF7519";

const DT_ARGS: &str = "($dst: bytes(1) *VARSIZE, byval $dd: i64, byval $df: i64, $src: bytes(1) *VARSIZE, byval $sd: i64, byval $sf: i64";

/// A duration unit's number from its special word.
#[must_use]
pub fn unit_of(word: &str) -> Option<u8> {
    UNITS.iter().find(|(w, _)| w.contains(&word)).map(|(_, u)| *u)
}

fn unit_word(e: &Expr) -> Option<u8> {
    match e {
        Expr::Special { word, index: None, .. } => unit_of(word),
        _ => None,
    }
}

fn special(e: Option<&Expr>) -> Option<&str> {
    match e {
        Some(Expr::Special { word, index: None, .. }) => Some(word),
        _ => None,
    }
}

/// The class of a date, time or timestamp type.
fn class_of(t: &Ty) -> Option<Class> {
    Some(match t {
        Ty::Date(..) => Class::Date,
        Ty::Time(..) => Class::Time,
        Ty::Timestamp(_) => Class::Stamp,
        _ => return None,
    })
}

/// A date literal's parts, when the text is a valid `*ISO` date.
fn iso_date(s: &str) -> Option<(u32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: u32 = s[..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..].parse().ok()?;
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let days = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return None,
    };
    (y >= 1 && d >= 1 && d <= days).then_some((y, m, d))
}

/// A time literal's parts (`hh.mm.ss`, or with colons).
fn iso_time(s: &str) -> Option<(u32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 8 || !matches!(b[2], b'.' | b':') || b[5] != b[2] {
        return None;
    }
    let h: u32 = s[..2].parse().ok()?;
    let m: u32 = s[3..5].parse().ok()?;
    let x: u32 = s[6..].parse().ok()?;
    (h < 24 && m < 60 && x < 60 || h == 24 && m == 0 && x == 0).then_some((h, m, x))
}

/// A timestamp literal normalized to `yyyy-mm-dd-hh.mm.ss[.f…]` and its
/// fractional digits.
fn iso_stamp(s: &str) -> Option<(String, u8)> {
    if s.len() < 19 || s.as_bytes()[10] != b'-' {
        return None;
    }
    iso_date(&s[..10])?;
    let t = s[11..19].replace(':', ".");
    iso_time(&t)?;
    let frac = match &s[19..] {
        "" => "",
        r => r.strip_prefix('.')?,
    };
    if frac.len() > 12 || !frac.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let p = u8::try_from(frac.len()).ok()?;
    let mut out = format!("{}-{t}", &s[..10]);
    if p > 0 {
        out.push('.');
        out.push_str(frac);
    }
    Some((out, p))
}

/// The characters of a figurative constant in a type's format.
fn fig_text(t: &Ty, f: &Fig) -> Option<String> {
    let hi = matches!(f, Fig::Hival);
    if !matches!(f, Fig::Hival | Fig::Loval) {
        return None;
    }
    Some(match t {
        Ty::Date(fmt, sep) => {
            let (lo, top) = dtfmt::years(*fmt);
            if hi {
                dtfmt::render_date(*fmt, *sep, top, 12, 31)
            } else {
                dtfmt::render_date(*fmt, *sep, lo, 1, 1)
            }
        }
        Ty::Time(fmt, sep) => {
            if hi {
                dtfmt::render_time(*fmt, *sep, 24, 0, 0)
            } else {
                dtfmt::render_time(*fmt, *sep, 0, 0, 0)
            }
        }
        Ty::Timestamp(p) => {
            let base = if hi { "9999-12-31-24.00.00" } else { "0001-01-01-00.00.00" };
            if *p == 0 { base.to_string() } else { format!("{base}.{}", "0".repeat(usize::from(*p))) }
        }
        _ => return None,
    })
}

impl Gen<'_> {
    fn dt_procs(&mut self) {
        self.rt_proc("_QRNX_DTCVT", &format!("{DT_ARGS})"));
        self.rt_proc("_QRNX_DTCMP", &format!("{DT_ARGS}) -> i32"));
        self.rt_proc("_QRNX_DTADD", &format!("{DT_ARGS}, byval $n: i64, byval $unit: i32)"));
        self.rt_proc("_QRNX_DTDIFF", &format!("{DT_ARGS}, byval $unit: i32) -> i64"));
        self.rt_proc(
            "_QRNX_DTSUB",
            "($src: bytes(1) *VARSIZE, byval $sd: i64, byval $sf: i64, byval $unit: i32) -> i64",
        );
        self.rt_proc("_QRNX_DTTEST", "($src: bytes(1) *VARSIZE, byval $sd: i64, byval $sf: i64) -> i32");
        self.rt_proc("_QRNX_DTNOW", "($dst: bytes(1) *VARSIZE, byval $dd: i64, byval $df: i64)");
    }

    /// An operand in storage with its descriptor and format word; `fmt`
    /// is the character or numeric form's format.
    fn dt_operand(&mut self, v: &V, fmt: i64) -> (String, i64, i64) {
        let v = if v.ty.is_numeric() {
            self.num_place(v)
        } else if v.kind == Vk::Place {
            v.clone()
        } else {
            let n = v.ty.str_len().unwrap_or(1).max(1);
            let t = self.b.temp(&format!("char({n})"));
            self.b.ins(format!("mov {t}, {}", v.op));
            V::place(Ty::Char(n), t)
        };
        let f = if v.ty.is_datetime() { dtfmt::of_type(&v.ty) } else { fmt };
        let d = self.desc_of(&v);
        (v.op, d, f)
    }

    /// `d'…'`, `t'…'`, `z'…'`: a temporary in `*ISO` form.
    pub fn dt_literal(&mut self, l: &Lit) -> Option<V> {
        let (ty, text) = match l {
            Lit::Date(s) => {
                let (f, sep) = (self.ctl.datfmt, self.ctl.datsep.unwrap_or(self.ctl.datfmt.sep()));
                if dtfmt::parse_date(f, sep, s).is_none() {
                    self.err(ids::BAD_DATE_LIT, 20, s.clone());
                    return None;
                }
                (Ty::Date(f, sep), s.clone())
            }
            Lit::Time(s) => {
                let (f, sep) = (self.ctl.timfmt, self.ctl.timsep.unwrap_or(self.ctl.timfmt.sep()));
                let Some((h, m, x)) = dtfmt::parse_time(f, sep, s) else {
                    self.err(ids::BAD_TIME_LIT, 20, s.clone());
                    return None;
                };
                (Ty::Time(f, sep), dtfmt::render_time(f, sep, h, m, x))
            }
            Lit::Timestamp(s) => {
                let Some((t, p)) = iso_stamp(s) else {
                    self.err(ids::BAD_TIMESTAMP_LIT, 20, s.clone());
                    return None;
                };
                (Ty::Timestamp(p), t)
            }
            _ => return None,
        };
        let t = self.b.temp(&self.ir_ty(&ty));
        self.b.ins(format!("mov {t}, {}", quote(&text)));
        Some(V::place(ty, t))
    }

    /// The type of an expression's date or time result: `*ISO`.
    #[must_use]
    pub fn dt_result(t: &Ty) -> Ty {
        match t {
            Ty::Date(..) => Ty::Date(DatFmt::Iso, '-'),
            Ty::Time(..) => Ty::Time(TimFmt::Iso, '.'),
            t => t.clone(),
        }
    }

    /// The default value of a date, time or timestamp: `*LOVAL`.
    #[must_use]
    pub fn dt_default(t: &Ty) -> Option<String> {
        fig_text(t, &Fig::Loval).map(|s| quote(&s))
    }

    /// Assign to a date, time or timestamp place.
    pub fn dt_assign(&mut self, dst: &V, src: &V) {
        if let Some(f) = &src.fig {
            match fig_text(&dst.ty, f) {
                Some(s) => self.b.ins(format!("mov {}, {}", dst.op, quote(&s))),
                None => self.err(ids::ASSIGN_TYPE, 30, ""),
            }
            return;
        }
        if !src.ty.is_datetime() || class_of(&src.ty) != class_of(&dst.ty) {
            self.err(ids::ASSIGN_TYPE, 30, "");
            return;
        }
        if src.ty == dst.ty && src.kind == Vk::Place {
            self.b.ins(format!("mov {}, {}", dst.op, src.op));
            return;
        }
        self.dt_convert(dst, src, 0);
    }

    /// `_QRNX_DTCVT` from `src` to `dst`; `fmt` is the format of
    /// whichever side is character or numeric.
    fn dt_convert(&mut self, dst: &V, src: &V, fmt: i64) {
        self.dt_procs();
        let (s, sd, sf) = self.dt_operand(src, fmt);
        let dd = self.desc_of(dst);
        let df = if dst.ty.is_datetime() { dtfmt::of_type(&dst.ty) } else { fmt };
        self.b.ins(format!("callb _QRNX_DTCVT({}, {dd}, {df}, {s}, {sd}, {sf})", dst.op));
    }

    /// A comparison with a date, time or timestamp operand.
    pub fn dt_compare(&mut self, c: &str, va: &V, vb: &V) -> Option<V> {
        let fix = |g: &mut Self, v: &V, other: &V| -> Option<V> {
            if v.fig.is_none() {
                return Some(v.clone());
            }
            let t = g.b.temp(&g.ir_ty(&other.ty));
            let tv = V::place(other.ty.clone(), t);
            g.dt_assign(&tv, v);
            Some(tv)
        };
        let a = fix(self, va, vb)?;
        let b = fix(self, vb, va)?;
        let (ca, cb) = (class_of(&a.ty), class_of(&b.ty));
        let mixed = matches!((ca, cb), (Some(Class::Date), Some(Class::Stamp)) | (Some(Class::Stamp), Some(Class::Date)));
        if ca.is_none() || cb.is_none() || ca != cb && !mixed {
            self.err(ids::TYPE_MISMATCH, 30, "");
            return None;
        }
        self.dt_procs();
        let (x, xd, xf) = self.dt_operand(&a, 0);
        let (y, yd, yf) = self.dt_operand(&b, 0);
        let r = self.b.reg();
        self.b.ins(format!("{r} = callb _QRNX_DTCMP({x}, {xd}, {xf}, {y}, {yd}, {yf})"));
        let i = self.b.reg();
        self.b.ins(format!("{i} = cmp.{c} {r}, 0"));
        Some(V::reg(Ty::Ind, i))
    }

    /// `+` and `-` with a date, time or timestamp and a duration.
    pub fn dt_arith(&mut self, op: BinOp, va: &V, vb: &V) -> Option<V> {
        let (dt, dur) = match (&va.ty, &vb.ty) {
            (t, Ty::Dur(_)) if t.is_datetime() && matches!(op, BinOp::Add | BinOp::Sub) => (va, vb),
            (Ty::Dur(_), t) if t.is_datetime() && op == BinOp::Add => (vb, va),
            _ => {
                self.err(ids::TYPE_MISMATCH, 30, "");
                return None;
            }
        };
        let Ty::Dur(u) = dur.ty else { return None };
        // A time takes hours, minutes and seconds only.
        if matches!(dt.ty, Ty::Time(..)) && u <= 2 {
            self.err(ids::TIME_DURATION, 20, "");
            return None;
        }
        let n = if op == BinOp::Sub {
            let r = self.b.reg();
            self.b.ins(format!("{r} = sub.chk 0, {}", dur.op));
            r
        } else {
            dur.op.clone()
        };
        let ty = Self::dt_result(&dt.ty);
        let t = self.b.temp(&self.ir_ty(&ty));
        let out = V::place(ty, t);
        self.dt_add_into(&out, dt, &n, u);
        Some(out)
    }

    fn dt_add_into(&mut self, dst: &V, src: &V, n: &str, unit: u8) {
        self.dt_procs();
        let (s, sd, sf) = self.dt_operand(src, 0);
        let dd = self.desc_of(dst);
        let df = dtfmt::of_type(&dst.ty);
        self.b.ins(format!("callb _QRNX_DTADD({}, {dd}, {df}, {s}, {sd}, {sf}, {n}, {unit})", dst.op));
    }

    /// The format word a format argument names for a class; the
    /// default when there is none.
    fn fmt_arg(&mut self, e: Option<&Expr>, class: Class, dft: Conv) -> Option<i64> {
        let c = match e {
            None | Some(Expr::Empty(_)) => dft,
            Some(Expr::Special { word, index: None, .. }) => {
                let Some(c) = dtfmt::parse(word, class) else {
                    // A format name that does not fit, or a word that
                    // names no format at all.
                    let root = word.trim_start_matches('*').trim_end_matches(['/', '-', '.', ',', '&', '0', ':']);
                    let formats = ["MDY", "DMY", "YMD", "JUL", "ISO", "USA", "EUR", "JIS", "CYMD", "CMDY", "CDMY", "LONGJUL", "HMS", "JOBRUN"];
                    if formats.contains(&root.to_ascii_uppercase().as_str()) {
                        self.err(BAD_FORMAT, 20, word.clone());
                    } else {
                        self.err(ids::UNKNOWN_BIF, 20, word.clone());
                        self.err(rpgfront::diag::ids::MISSING_OPERAND, 30, "");
                    }
                    return None;
                };
                c
            }
            Some(_) => {
                self.err(ids::BIF_ARGS, 30, "");
                return None;
            }
        };
        Some(dtfmt::word(c, 0))
    }

    fn ctl_date(&self) -> Conv {
        Conv {
            code: dtfmt::date_code(self.ctl.datfmt),
            sep: match self.ctl.datsep {
                Some('\0') => None,
                Some(c) => Some(c),
                None => Some(self.ctl.datfmt.sep()),
            },
        }
    }

    fn ctl_time(&self) -> Conv {
        Conv {
            code: dtfmt::time_code(self.ctl.timfmt),
            sep: match self.ctl.timsep {
                Some('\0') => None,
                Some(c) => Some(c),
                None => Some(self.ctl.timfmt.sep()),
            },
        }
    }

    /// The built-in functions of dates, times and timestamps; `None`
    /// when `name` is not one of them.
    pub fn dt_bif(&mut self, name: &str, args: &[Expr]) -> Option<Option<V>> {
        Some(match name {
            "%DATE" | "%TIME" | "%TIMESTAMP" => self.dt_make(name, args),
            "%YEARS" | "%MONTHS" | "%DAYS" | "%HOURS" | "%MINUTES" | "%SECONDS" | "%MSECONDS" => {
                let u = unit_of(&format!("*{}", &name[1..]));
                let n = args.first().and_then(|a| self.expr(a, Want::default()));
                // The count is a number.
                if let Some(v) = &n
                    && !v.ty.is_numeric()
                {
                    self.err(ids::DURATION_PARM, 20, name.to_string());
                    return Some(None);
                }
                match (u, n, args.len()) {
                    (Some(u), Some(v), 1) => self.int_reg(&v).map(|r| V::reg(Ty::Dur(u), r)),
                    _ => {
                        self.err(ids::BIF_ARGS, 30, name.to_string());
                        None
                    }
                }
            }
            "%DIFF" => self.dt_diff(args),
            "%SUBDT" => self.dt_subdt(args),
            _ => return None,
        })
    }

    fn dt_make(&mut self, name: &str, args: &[Expr]) -> Option<V> {
        let (class, ty) = match name {
            "%DATE" => (Class::Date, Ty::Date(DatFmt::Iso, '-')),
            "%TIME" => (Class::Time, Ty::Time(TimFmt::Iso, '.')),
            _ => {
                let p = match args.get(2) {
                    Some(Expr::Lit(Lit::Num(n), _)) => n.parse().unwrap_or(6),
                    _ => 6,
                };
                (Class::Stamp, Ty::Timestamp(p))
            }
        };
        let t = self.b.temp(&self.ir_ty(&ty));
        let out = V::place(ty, t);
        let now = args.is_empty()
            || matches!(args.first(), Some(Expr::Empty(_)))
            || special(args.first()) == Some("*SYS");
        if now {
            self.dt_now(&out);
            return Some(out);
        }
        let v = self.expr(&args[0], Want::default())?;
        let v = self.as_chars(&v);
        if v.ty.is_datetime() {
            let from = class_of(&v.ty);
            let ok = match class {
                Class::Date => matches!(from, Some(Class::Date | Class::Stamp)),
                Class::Time => matches!(from, Some(Class::Time | Class::Stamp)),
                Class::Stamp => matches!(from, Some(Class::Date | Class::Stamp)),
            };
            if !ok {
                self.err(ids::BIF_ARGS, 30, name.to_string());
                return None;
            }
            self.dt_convert(&out, &v, 0);
            return Some(out);
        }
        if !v.ty.is_char() && !v.ty.is_numeric() {
            self.err(ids::BIF_ARGS, 30, name.to_string());
            return None;
        }
        // Without a format, character and numeric data are *ISO.
        let dft = match class {
            Class::Date => Conv { code: 1, sep: Some('-') },
            Class::Time => Conv { code: 33, sep: Some('.') },
            Class::Stamp => Conv { code: dtfmt::STAMP, sep: Some('-') },
        };
        let mut f = self.fmt_arg(args.get(1), class, dft)?;
        let sep = u8::try_from((f >> 8) & 0xFF).unwrap_or(0);
        let c = Conv {
            code: u8::try_from(f & 0xFF).unwrap_or(1),
            sep: (sep != 0).then_some(char::from(sep)),
        };
        // An operand shorter than the format is refused.
        let (have, want) = match (&args[0], &v.ty) {
            (Expr::Lit(Lit::Char(s), _), _) => (s.chars().count(), dtfmt::conv_len(c, class)),
            (Expr::Lit(Lit::Num(n), _), _) => (n.chars().filter(char::is_ascii_digit).count(), dtfmt::conv_len(Conv { sep: None, ..c }, class)),
            (_, Ty::Char(n)) => (usize::try_from(*n).unwrap_or(0), dtfmt::conv_len(c, class)),
            (_, t) if t.is_numeric() => (usize::from(t.shape().map_or(63, |s| s.0)), dtfmt::conv_len(Conv { sep: None, ..c }, class)),
            _ => (0, 0),
        };
        if have < want {
            let what = match &args[0] {
                Expr::Lit(Lit::Char(s), _) => s.clone(),
                Expr::Lit(Lit::Num(n), _) => n.clone(),
                e => e.name().unwrap_or_default().to_string(),
            };
            match class {
                Class::Date => self.err(SHORT_DATE, 30, what),
                Class::Stamp => self.err(SHORT_STAMP, 30, what),
                Class::Time => {}
            }
        }
        if class == Class::Stamp && v.ty.is_numeric() {
            // A number holds the digits and six fractional ones.
            f = dtfmt::word(Conv { code: dtfmt::STAMP, sep: None }, 6);
        }
        self.dt_convert(&out, &v, f);
        Some(out)
    }

    /// The current value into a place.
    pub fn dt_now(&mut self, dst: &V) {
        self.dt_procs();
        let dd = self.desc_of(dst);
        let df = if dst.ty.is_datetime() {
            dtfmt::of_type(&dst.ty)
        } else {
            dtfmt::word(Conv { code: 32, sep: None }, 0)
        };
        self.b.ins(format!("callb _QRNX_DTNOW({}, {dd}, {df})", dst.op));
    }

    /// The current date into a numeric field in format `code`, without
    /// separators.
    pub fn dt_now_as(&mut self, dst: &V, code: u8) {
        self.dt_procs();
        let dd = self.desc_of(dst);
        let w = dtfmt::word(Conv { code, sep: None }, 0);
        self.b.ins(format!("callb _QRNX_DTNOW({}, {dd}, {w})", dst.op));
    }

    fn dt_value(&mut self, e: &Expr) -> Option<V> {
        let v = self.expr(e, Want::default())?;
        if !v.ty.is_datetime() {
            self.err(ids::BIF_ARGS, 30, "");
            return None;
        }
        Some(v)
    }

    fn dt_diff(&mut self, args: &[Expr]) -> Option<V> {
        if args.len() != 3 {
            self.err(ids::BIF_ARGS, 30, "%DIFF");
            return None;
        }
        // Both values are dates, times or timestamps.
        let a = self.expr(&args[0], Want::default())?;
        let b = self.expr(&args[1], Want::default())?;
        let mut bad = false;
        for (v, id) in [(&a, ids::DIFF_FIRST), (&b, ids::DIFF_SECOND)] {
            if !v.ty.is_datetime() {
                self.err(id, 20, "%DIFF");
                bad = true;
            }
        }
        if bad {
            return None;
        }
        let Some(u) = unit_word(&args[2]) else {
            self.err(ids::BIF_ARGS, 30, "%DIFF");
            return None;
        };
        self.dt_procs();
        let (x, xd, xf) = self.dt_operand(&a, 0);
        let (y, yd, yf) = self.dt_operand(&b, 0);
        let r = self.b.reg();
        self.b.ins(format!("{r} = callb _QRNX_DTDIFF({x}, {xd}, {xf}, {y}, {yd}, {yf}, {u})"));
        Some(V::reg(Ty::Int(20), r))
    }

    fn dt_subdt(&mut self, args: &[Expr]) -> Option<V> {
        let (Some(v), Some(Some(u))) = (args.first(), args.get(1).map(unit_word)) else {
            self.err(ids::BIF_ARGS, 30, "%SUBDT");
            return None;
        };
        let v = self.dt_value(v)?;
        Some(self.dt_part(&v, u))
    }

    fn dt_part(&mut self, v: &V, u: u8) -> V {
        self.dt_procs();
        let (x, xd, xf) = self.dt_operand(v, 0);
        let r = self.b.reg();
        self.b.ins(format!("{r} = callb _QRNX_DTSUB({x}, {xd}, {xf}, {u})"));
        V::reg(Ty::Int(20), r)
    }

    /// `%CHAR` of a date, time or timestamp.
    pub fn dt_char(&mut self, v: &V, fmt: Option<&Expr>) -> Option<V> {
        let class = class_of(&v.ty)?;
        let own = match class {
            Class::Stamp => Conv { code: dtfmt::STAMP, sep: Some('-') },
            _ => {
                let w = dtfmt::of_type(&v.ty);
                let sep = u8::try_from((w >> 8) & 0xFF).unwrap_or(0);
                Conv {
                    code: u8::try_from(w & 0xFF).unwrap_or(1),
                    sep: (sep != 0).then_some(char::from(sep)),
                }
            }
        };
        let f = self.fmt_arg(fmt, class, own)?;
        let frac = if let Ty::Timestamp(p) = v.ty { p } else { 0 };
        let f = f | i64::from(frac) << 16;
        let t = self.b.temp(&self.ir_ty(&Ty::VarChar(40, 2)));
        let out = V::place(Ty::VarChar(40, 2), t);
        if fmt.is_none() && v.kind == Vk::Place {
            // In its own format a value is its characters.
            self.b.ins(format!("mov {}, {}", out.op, v.op));
        } else {
            self.dt_convert(&out, v, f);
        }
        Some(out)
    }

    /// `%DEC` of a date, time or timestamp: its digits in a format.
    pub fn dt_dec(&mut self, v: &V, fmt: Option<&Expr>) -> Option<V> {
        let class = class_of(&v.ty)?;
        let own = dtfmt::of_type(&v.ty);
        let own = Conv {
            code: u8::try_from(own & 0xFF).unwrap_or(1),
            sep: None,
        };
        let f = self.fmt_arg(fmt, class, own)?;
        let code = u8::try_from(f & 0xFF).unwrap_or(1);
        let frac = if let Ty::Timestamp(p) = v.ty { p } else { 0 };
        let digits: u8 = match code {
            1..=4 => 8,
            8 => 5,
            9..=12 => 7,
            dtfmt::STAMP => 14 + frac,
            _ => 6,
        };
        let ty = Ty::Packed(digits, 0);
        let t = self.b.temp(&self.ir_ty(&ty));
        let out = V::place(ty, t);
        self.dt_convert(&out, v, (f & !0xFF00) | i64::from(frac) << 16);
        Some(out)
    }

    /// `INZ(*SYS)` and `INZ(*JOB)` of a date, time or timestamp.
    pub fn dt_inz(&mut self, ty: &Ty, place: &str, e: &Expr) -> bool {
        if !ty.is_datetime() || !matches!(special(Some(e)), Some("*SYS" | "*JOB")) {
            return false;
        }
        self.dt_now(&V::place(ty.clone(), place));
        true
    }

    /// The date, time and timestamp operations: `TEST`, `ADDDUR`,
    /// `SUBDUR`, `EXTRCT`, `TIME`. False when `base` is none of them.
    pub fn dt_op(&mut self, base: &str, op: &Op) -> bool {
        match base {
            "TEST" => self.dt_test(op),
            "ADDDUR" | "SUBDUR" => self.dt_dur_op(base == "SUBDUR", op),
            "EXTRCT" => {
                let (Some(v), Some(Some(u)), Some(r)) = (op.f2.first(), op.f2.get(1).map(unit_word), op.result.first())
                else {
                    self.err(ids::BIF_ARGS, 30, "EXTRCT");
                    return true;
                };
                let (Some(v), Some(t)) = (self.dt_value(v), self.target(r)) else { return true };
                let p = self.dt_part(&v, u);
                self.assign(&t, &p, false);
            }
            "TIME" => {
                let Some(r) = op.result.first() else { return true };
                let Some(t) = self.target(r) else {
                    // An undefined result is not a field TIME can set.
                    if r.name().is_some_and(|n| self.t.find(self.scope, n).is_none()) {
                        self.err(ids::TIME_RESULT, 30, String::new());
                    }
                    return true;
                };
                self.dt_time_op(&t);
            }
            _ => return false,
        }
        true
    }

    fn dt_test(&mut self, op: &Op) {
        let Some(r) = op.result.first() else { return };
        let Some(v) = self.expr(r, Want::default()) else { return };
        let class = if op.ext.contains('D') {
            Some(Class::Date)
        } else if op.ext.contains('T') {
            Some(Class::Time)
        } else if op.ext.contains('Z') {
            Some(Class::Stamp)
        } else {
            None
        };
        let f = match class {
            None => {
                if !v.ty.is_datetime() {
                    self.err(ids::TYPE_MISMATCH, 30, "TEST");
                    return;
                }
                0
            }
            Some(c) => {
                if !v.ty.is_char() && !v.ty.is_numeric() {
                    self.err(ids::TYPE_MISMATCH, 30, "TEST");
                    return;
                }
                let dft = match c {
                    Class::Date => self.ctl_date(),
                    Class::Time => self.ctl_time(),
                    Class::Stamp => Conv { code: dtfmt::STAMP, sep: Some('-') },
                };
                let Some(mut f) = self.fmt_arg(op.f1.first(), c, dft) else { return };
                if c == Class::Stamp && v.ty.is_numeric() {
                    f = dtfmt::word(Conv { code: dtfmt::STAMP, sep: None }, 6);
                }
                f
            }
        };
        self.dt_procs();
        let (x, xd, xf) = self.dt_operand(&v, f);
        let r = self.b.reg();
        self.b.ins(format!("{r} = callb _QRNX_DTTEST({x}, {xd}, {xf})"));
        let bad = self.b.reg();
        self.b.ins(format!("{bad} = cmp.eq {r}, 0"));
        if !op.resind[1].is_empty() {
            let ind = op.resind[1].clone();
            self.set_ind(&ind, &bad);
        }
    }

    fn dt_dur_op(&mut self, sub: bool, op: &Op) {
        let (Some(r0), Some(f2)) = (op.result.first(), op.f2.first()) else { return };
        // SUBDUR's difference form: `date SUBDUR date result:unit`.
        if sub && let Some(u) = op.result.get(1).and_then(unit_word) {
            let a = op.f1.first().cloned().unwrap_or_else(|| r0.clone());
            let (Some(x), Some(y), Some(t)) = (self.dt_value(&a), self.dt_value(f2), self.target(r0)) else { return };
            self.dt_procs();
            let (xa, xd, xf) = self.dt_operand(&x, 0);
            let (ya, yd, yf) = self.dt_operand(&y, 0);
            let r = self.b.reg();
            self.b.ins(format!("{r} = callb _QRNX_DTDIFF({xa}, {xd}, {xf}, {ya}, {yd}, {yf}, {u})"));
            self.assign(&t, &V::reg(Ty::Int(20), r), false);
            return;
        }
        let Some(u) = op.f2.get(1).and_then(unit_word) else {
            self.err(ids::BIF_ARGS, 30, op.code.clone());
            return;
        };
        let base = op.f1.first().cloned().unwrap_or_else(|| r0.clone());
        let (Some(src), Some(t)) = (self.dt_value(&base), self.target(r0)) else { return };
        let Some(nv) = self.expr(f2, Want::default()) else { return };
        let Some(n) = self.int_reg(&nv) else { return };
        let n = if sub {
            let m = self.b.reg();
            self.b.ins(format!("{m} = sub.chk 0, {n}"));
            m
        } else {
            n
        };
        if !t.ty.is_datetime() {
            self.err(ids::TYPE_MISMATCH, 30, op.code.clone());
            return;
        }
        self.dt_add_into(&t, &src, &n, u);
    }

    /// `TIME`: the time, or time and date, into a field.
    fn dt_time_op(&mut self, t: &V) {
        if t.ty.is_datetime() {
            self.dt_now(t);
            return;
        }
        let Some((d, _)) = t.ty.shape() else {
            self.err(ids::TIME_RESULT, 30, "TIME");
            return;
        };
        let hms = V::place(Ty::Zoned(6, 0), self.b.temp("zoned(6,0)"));
        self.dt_now(&hms);
        if d <= 6 {
            self.assign(t, &hms, false);
            return;
        }
        // hhmmss then the date in the job's format (QS/400: *MDY).
        let (dd, f) = if d >= 14 { (8, 2) } else { (6, 5) };
        let date = V::place(Ty::Zoned(dd, 0), self.b.temp(&format!("zoned({dd},0)")));
        self.dt_procs();
        let desc = self.desc_of(&date);
        let w = dtfmt::word(Conv { code: f, sep: None }, 0);
        self.b.ins(format!("callb _QRNX_DTNOW({}, {desc}, {w})", date.op));
        let scale = V::lit(Ty::Packed(9, 0), if dd == 8 { "100000000" } else { "1000000" });
        let Some(hi) = self.arith(BinOp::Mul, &hms, &scale, Want::default()) else { return };
        if let Some(sum) = self.arith(BinOp::Add, &hi, &date, Want::default()) {
            self.assign(t, &sum, false);
        }
    }
}
