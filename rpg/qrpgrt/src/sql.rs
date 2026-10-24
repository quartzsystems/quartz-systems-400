// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Embedded SQL at run time. Generated code names a statement's input
//! host variables (`_QRNX_SQLIN`) and output host variables
//! (`_QRNX_SQLOUT`), then runs it (`_QRNX_SQLRUN`): the inputs are read
//! as SQL values, the job's [`sql::embed::Embedded`] runs the
//! statement, the row's values are converted into the outputs (null
//! into an indicator, truncation and overflow as IBM reports them) and
//! the SQLCA is written with IBM's layout.

use std::cell::RefCell;

use codegen::abi::Env;
use db::record::datetime::{Date, Time};
use dds::{DateFmt, TimeFmt};
use qdecimal::{Decimal, Rounding, Shape};
use sql::embed::{Desc as SqlDesc, Request};
use sql::value::Timestamp;
use sql::{Sqlca, Value};

use crate::value::{Desc, Kind, bytes, ebcdic, read_f64, read_num};

/// An output host variable.
#[derive(Clone, Copy)]
struct Out {
    addr: u64,
    desc: Desc,
    extra: i64,
    ind: Option<u64>,
    stride: i64,
}

/// The statement being put together.
#[derive(Default)]
struct Pending {
    inputs: Vec<Value>,
    outs: Vec<Out>,
}

thread_local! {
    static PENDING: RefCell<Pending> = RefCell::new(Pending::default());
}

const BLANK: u8 = 0x40;

fn text_of(addr: u64, len: usize) -> String {
    ccsid::from_ebcdic(bytes(addr, len))
}

fn date_fmt(extra: i64) -> (DateFmt, Option<char>) {
    let f = match extra & 0xFF {
        2 => DateFmt::Usa,
        3 => DateFmt::Eur,
        4 => DateFmt::Jis,
        5 => DateFmt::Mdy,
        6 => DateFmt::Dmy,
        7 => DateFmt::Ymd,
        8 => DateFmt::Jul,
        _ => DateFmt::Iso,
    };
    (f, char::from_u32(u32::try_from(extra >> 8).unwrap_or(0)).filter(|c| *c != '\0'))
}

fn time_fmt(extra: i64) -> (TimeFmt, Option<char>) {
    let f = match extra & 0xFF {
        2 => TimeFmt::Hms,
        3 => TimeFmt::Usa,
        4 => TimeFmt::Eur,
        5 => TimeFmt::Jis,
        _ => TimeFmt::Iso,
    };
    (f, char::from_u32(u32::try_from(extra >> 8).unwrap_or(0)).filter(|c| *c != '\0'))
}

/// A host variable's value.
fn read_value(addr: u64, d: &Desc, extra: i64) -> Value {
    match d.kind {
        Kind::Char => Value::Str(text_of(addr, d.size())),
        Kind::VarChar => {
            let b = bytes(addr, 2);
            let n = usize::from(u16::from_be_bytes([b[0], b[1]])).min(d.len as usize);
            Value::Str(ccsid::from_ebcdic(&bytes(addr, 2 + n)[2..]))
        }
        Kind::Ind => Value::Str(text_of(addr, 1)),
        Kind::Float => {
            let f = read_f64(addr, d).unwrap_or(0.0);
            Decimal::parse(&format!("{f}")).map_or(Value::Null, Value::Num)
        }
        Kind::Date => {
            let (f, _) = date_fmt(extra);
            Date::parse(&text_of(addr, d.size()), f).map_or(Value::Null, Value::Date)
        }
        Kind::Time => {
            let (f, _) = time_fmt(extra);
            Time::parse(&text_of(addr, d.size()), f).map_or(Value::Null, Value::Time)
        }
        Kind::Timestamp => Timestamp::parse(&text_of(addr, d.size())).map_or(Value::Null, Value::Timestamp),
        _ => read_num(addr, d).map_or(Value::Null, Value::Num),
    }
}

/// An indicator's value (a 2-byte integer).
fn read_ind(addr: u64) -> i16 {
    let b = bytes(addr, 2);
    i16::from_be_bytes([b[0], b[1]])
}

fn write_ind(addr: u64, v: i16) {
    bytes(addr, 2).copy_from_slice(&v.to_be_bytes());
}

/// `_QRNX_SQLIN(var, desc, extra, ind, has_ind)`: an input host variable.
pub extern "sysv64" fn sql_in(_env: *mut Env, _argc: i64, _od: i64, var: i64, desc: i64, extra: i64, ind: i64, has: i64) {
    let d = Desc::of(desc);
    let v = if has != 0 && read_ind(ind.cast_unsigned()) < 0 {
        Value::Null
    } else {
        read_value(var.cast_unsigned(), &d, extra)
    };
    PENDING.with(|p| p.borrow_mut().inputs.push(v));
}

/// `_QRNX_SQLOUT(var, desc, extra, ind, has_ind, stride)`: an output
/// host variable; `stride` is a structure array's element size.
pub extern "sysv64" fn sql_out(_env: *mut Env, _argc: i64, _od: i64, var: i64, desc: i64, extra: i64, ind: i64, has: i64, stride: i64) {
    let o = Out {
        addr: var.cast_unsigned(),
        desc: Desc::of(desc),
        extra,
        ind: (has != 0).then_some(ind.cast_unsigned()),
        stride,
    };
    PENDING.with(|p| p.borrow_mut().outs.push(o));
}

/// What assigning a value found.
enum Put {
    /// Written as it is.
    Ok,
    /// Character data cut: the length it had.
    Truncated(usize),
}

/// The bytes a value takes in a host variable, or the SQLCODE that
/// stops the assignment.
fn image(v: &Value, d: &Desc, extra: i64) -> Result<(Vec<u8>, Put), (i32, &'static str)> {
    let n = d.size();
    match d.kind {
        Kind::Char | Kind::Ind | Kind::VarChar => {
            let text = match v {
                Value::Str(s) => s.clone(),
                Value::Date(x) => x.render(DateFmt::Iso, None),
                Value::Time(x) => x.render(TimeFmt::Iso, None),
                Value::Timestamp(t) => ts_text(t),
                Value::Num(x) => x.to_string(),
                Value::Null => String::new(),
            };
            let e = ebcdic(&text);
            let cap = if d.kind == Kind::VarChar { d.len as usize } else { n };
            let put = if e.len() > cap && e[cap..].iter().any(|b| *b != BLANK) { Put::Truncated(e.len()) } else { Put::Ok };
            let mut body: Vec<u8> = e.into_iter().take(cap).collect();
            if d.kind == Kind::VarChar {
                let mut out = u16::try_from(body.len()).unwrap_or(0).to_be_bytes().to_vec();
                out.append(&mut body);
                return Ok((out, put));
            }
            body.resize(n, BLANK);
            Ok((body, put))
        }
        Kind::Date => {
            let x = v.as_date().map_err(|_| (-303, "42806"))?;
            let (f, sep) = date_fmt(extra);
            Ok((ebcdic(&x.render(f, sep)), Put::Ok))
        }
        Kind::Time => {
            let x = v.as_time().map_err(|_| (-303, "42806"))?;
            let (f, sep) = time_fmt(extra);
            Ok((ebcdic(&x.stored(f, sep)), Put::Ok))
        }
        Kind::Timestamp => {
            let x = v.as_timestamp().map_err(|_| (-303, "42806"))?;
            let mut t = ebcdic(&ts_text(&x));
            t.resize(n, 0xF0);
            Ok((t, Put::Ok))
        }
        _ => {
            let x = v.as_decimal().map_err(|_| (-420, "22018"))?;
            num_image(&x, d).map(|b| (b, Put::Ok)).ok_or((-304, "22003"))
        }
    }
}

/// `yyyy-mm-dd-hh.mm.ss.ffffff`.
fn ts_text(t: &Timestamp) -> String {
    format!(
        "{}-{:02}.{:02}.{:02}.{:06}",
        t.date.render(DateFmt::Iso, None),
        t.time.hour,
        t.time.minute,
        t.time.second,
        t.micros
    )
}

/// A number in a numeric host variable's storage: fractional digits
/// past its scale dropped, `None` when the integer part does not fit.
fn num_image(v: &Decimal, d: &Desc) -> Option<Vec<u8>> {
    let size = d.size();
    // The value as the host variable holds it, checked to fit.
    match d.kind {
        Kind::Int | Kind::Uns => {
            let t = v.rescale(0, Rounding::Truncate).ok()?.to_i128()?;
            let bits = u32::try_from(size * 8).ok()?;
            let ok = if d.kind == Kind::Int {
                let lim = 1i128 << (bits - 1);
                (-lim..lim).contains(&t)
            } else {
                t >= 0 && t < (1i128 << bits)
            };
            if !ok {
                return None;
            }
            let be = t.to_be_bytes();
            return Some(be[16 - size..].to_vec());
        }
        Kind::Float => {
            let f: f64 = v.to_string().parse().ok()?;
            #[allow(clippy::cast_possible_truncation)]
            return Some(if d.len == 4 { (f as f32).to_be_bytes().to_vec() } else { f.to_be_bytes().to_vec() });
        }
        _ => {}
    }
    let digits = u8::try_from(d.len.clamp(1, 63)).ok()?;
    let shape = Shape::new(digits, d.dec).ok()?;
    let fitted = v.rescale(d.dec, Rounding::Truncate).ok()?.fit(shape, Rounding::Truncate).ok()?;
    let enc = match d.kind {
        Kind::Packed => qdecimal::Packed::new(digits, d.dec).and_then(|p| p.encode(&fitted, Rounding::Truncate)),
        Kind::Zoned => qdecimal::Zoned::new(digits, d.dec).and_then(|z| z.encode(&fitted, Rounding::Truncate)),
        Kind::Bin => qdecimal::Binary::new(digits, d.dec).and_then(|b| b.encode(&fitted, Rounding::Truncate)),
        _ => return None,
    };
    enc.ok()
}

/// A host variable's empty value: what a null leaves in it.
fn empty(d: &Desc, extra: i64) -> Vec<u8> {
    match d.kind {
        Kind::Char | Kind::Ind => vec![BLANK; d.size()],
        Kind::VarChar => vec![0, 0],
        Kind::Date => ebcdic(&Date::new(1, 1, 1).map_or(String::new(), |x| x.render(date_fmt(extra).0, date_fmt(extra).1))),
        Kind::Time => ebcdic("00.00.00"),
        Kind::Timestamp => {
            let mut t = ebcdic("0001-01-01-00.00.00.000000");
            t.resize(d.size(), 0xF0);
            t
        }
        _ => num_image(&Decimal::zero(), d).unwrap_or_else(|| vec![0; d.size()]),
    }
}

/// Write the SQLCA in IBM's layout (136 bytes).
fn write_sqlca(addr: u64, ca: &Sqlca) {
    let b = bytes(addr, 136);
    b[..8].copy_from_slice(&ebcdic("SQLCA   "));
    b[8..12].copy_from_slice(&136i32.to_be_bytes());
    b[12..16].copy_from_slice(&ca.sqlcode.to_be_bytes());
    let mc = ebcdic(&ca.sqlerrmc);
    let n = mc.len().min(70);
    b[16..18].copy_from_slice(&u16::try_from(n).unwrap_or(0).to_be_bytes());
    b[18..88].fill(BLANK);
    b[18..18 + n].copy_from_slice(&mc[..n]);
    let mut p = ebcdic(&ca.sqlerrp);
    p.resize(8, BLANK);
    b[88..96].copy_from_slice(&p[..8]);
    for (i, e) in ca.sqlerrd.iter().enumerate() {
        b[96 + i * 4..100 + i * 4].copy_from_slice(&e.to_be_bytes());
    }
    for (i, c) in ca.sqlwarn.iter().enumerate() {
        b[120 + i] = ccsid::byte_of(*c).unwrap_or(BLANK);
    }
    let mut st = ebcdic(&ca.sqlstate);
    st.resize(5, 0xF0);
    b[131..136].copy_from_slice(&st[..5]);
}

/// Assign a row to the outputs (`row` for an array of structures); the
/// SQLCA takes what the assignment found.
fn assign(outs: &[Out], row: &[Value], r: usize, ca: &mut Sqlca) {
    // Where each value goes, its bytes, its indicator and value.
    type Staged = (u64, Vec<u8>, Option<(u64, i16)>);
    let mut staged: Vec<Staged> = Vec::new();
    let mut truncated = false;
    for (k, o) in outs.iter().enumerate() {
        let Some(v) = row.get(k) else { break };
        let off = u64::try_from(o.stride.max(0)).unwrap_or(0) * r as u64;
        let ind = o.ind.map(|i| i + off);
        if v.is_null() {
            let Some(i) = ind else {
                *ca = Sqlca {
                    sqlcode: -305,
                    sqlstate: "22002".into(),
                    ..Sqlca::default()
                };
                return;
            };
            staged.push((o.addr + off, empty(&o.desc, o.extra), Some((i, -1))));
            continue;
        }
        match image(v, &o.desc, o.extra) {
            Ok((b, put)) => {
                let iv = match put {
                    Put::Ok => 0,
                    Put::Truncated(n) => {
                        truncated = true;
                        i16::try_from(n).unwrap_or(i16::MAX)
                    }
                };
                staged.push((o.addr + off, b, ind.map(|i| (i, iv))));
            }
            Err((code, state)) => {
                *ca = Sqlca {
                    sqlcode: code,
                    sqlstate: state.into(),
                    ..Sqlca::default()
                };
                return;
            }
        }
    }
    for (a, b, ind) in staged {
        bytes(a, b.len()).copy_from_slice(&b);
        if let Some((i, v)) = ind {
            write_ind(i, v);
        }
    }
    if truncated {
        ca.sqlwarn[0] = 'W';
        ca.sqlwarn[1] = 'W';
        if ca.sqlcode == 0 {
            ca.sqlstate = "01004".into();
        }
    }
}

/// `_QRNX_SQLRUN(sqlca, text, tlen, scope, slen)`: run the statement
/// the calls before named.
pub extern "sysv64" fn sql_run(env: *mut Env, _argc: i64, _od: i64, ca: i64, text: i64, tlen: i64, scope: i64, slen: i64) {
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    let desc_text = text_of(text.cast_unsigned(), usize::try_from(tlen).unwrap_or(0));
    let scope = text_of(scope.cast_unsigned(), usize::try_from(slen).unwrap_or(0)).trim().to_string();
    let Some(desc) = SqlDesc::decode(&desc_text) else {
        write_sqlca(ca.cast_unsigned(), &Sqlca::from_error(&sql::SqlError::Unsupported { what: "statement".into() }));
        return;
    };
    let job = mi::storage::job_of(env);
    let req = Request {
        scope,
        desc,
        inputs: pending.inputs,
    };
    let reply = match job.sql.as_mut() {
        Some(s) => s.run(&req),
        None => sql::embed::Reply {
            sqlca: Sqlca::from_error(&sql::SqlError::Database {
                what: "no database for embedded SQL in this job".into(),
                msgid: None,
            }),
            rows: Vec::new(),
        },
    };
    let mut sqlca = reply.sqlca;
    // A failed statement leaves the host variables alone, except that
    // more than one row still gives the first.
    if !sqlca.is_error() || sqlca.sqlcode == -811 {
        for (r, row) in reply.rows.iter().enumerate() {
            assign(&pending.outs, row, r, &mut sqlca);
            if sqlca.is_error() && sqlca.sqlcode != -811 {
                break;
            }
        }
    }
    if sqlca.sqlwarn.iter().skip(1).any(|c| *c == 'W') {
        sqlca.sqlwarn[0] = 'W';
    }
    write_sqlca(ca.cast_unsigned(), &sqlca);
}
