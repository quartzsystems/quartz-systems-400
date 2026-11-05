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
    /// The indicator array's element size.
    istride: i64,
}

/// The statement being put together.
#[derive(Default)]
struct Pending {
    inputs: Vec<Value>,
    /// Where each input came from, for a `CALL`'s outputs.
    in_vars: Vec<Out>,
    outs: Vec<Out>,
    /// A blocked INSERT's inputs: each a structure array.
    rows_in: Vec<Out>,
    /// The SQLDA a `DESCRIBE` or `PREPARE INTO` fills.
    da: Option<u64>,
    /// A file reference variable is named: BLOCKED(sql), the runtime has
    /// no IFS of its own to read or write the file with.
    file_ref: bool,
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

/// The mark the compiler puts on an `SQLTYPE` host variable's extra
/// word (`rpgc::codegen::sql::SQLTYPE_EXTRA`); the kind is below it: 1
/// a LOB, 3 a file reference, the data (1 character, 2 binary, 3
/// double-byte) in the next four bits.
const SQLTYPE_EXTRA: i64 = 1 << 40;

/// The `SQLTYPE` kind of a host variable, when it has one.
fn sqltype(extra: i64) -> Option<i64> {
    (extra & SQLTYPE_EXTRA != 0).then_some(extra & 0xFF)
}

fn be32(addr: u64) -> u32 {
    let b = bytes(addr, 4);
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// A LOB host variable's value: its length, then its data.
fn read_lob(addr: u64, d: &Desc, kind: i64) -> Value {
    let cap = d.size().saturating_sub(4);
    // A DBCLOB's length counts its double-byte characters.
    let unit = if kind >> 4 == 3 { 2 } else { 1 };
    let n = (usize::try_from(be32(addr)).unwrap_or(0) * unit).min(cap);
    let data = bytes(addr + 4, n);
    match kind >> 4 {
        2 => Value::Bin(data.to_vec()),
        3 => Value::Str(String::from_utf16_lossy(&data.chunks(2).map(|c| u16::from_be_bytes([c[0], c.get(1).copied().unwrap_or(0)])).collect::<Vec<_>>())),
        _ => Value::Str(ccsid::from_ebcdic(data)),
    }
}

/// A value as a LOB host variable holds it.
fn lob_image(v: &Value, d: &Desc, kind: i64) -> (Vec<u8>, Put) {
    let cap = d.size().saturating_sub(4);
    let data: Vec<u8> = match (v, kind >> 4) {
        (Value::Bin(b), _) => b.clone(),
        (v, 3) => v.as_text().encode_utf16().flat_map(u16::to_be_bytes).collect(),
        (v, _) => ebcdic(&v.as_text()),
    };
    let put = if data.len() > cap { Put::Truncated(data.len()) } else { Put::Ok };
    let unit = if kind >> 4 == 3 { 2 } else { 1 };
    let mut out = u32::try_from(data.len().min(cap) / unit).unwrap_or(0).to_be_bytes().to_vec();
    out.extend_from_slice(&data[..data.len().min(cap)]);
    (out, put)
}

/// A host variable's value.
fn read_value(addr: u64, d: &Desc, extra: i64) -> Value {
    if let Some(k) = sqltype(extra) {
        return read_lob(addr, d, k);
    }
    match d.kind {
        Kind::Char => Value::Str(text_of(addr, d.size())),
        Kind::VarChar => Value::Str(ccsid::from_ebcdic(d.var_data(bytes(addr, d.size())))),
        Kind::Ind => Value::Str(text_of(addr, 1)),
        Kind::Float => Value::Double(read_f64(addr, d).unwrap_or(0.0)),
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
    if sqltype(extra).is_some_and(|k| k & 0xF == 3) {
        PENDING.with(|p| p.borrow_mut().file_ref = true);
    }
    let d = Desc::of(desc);
    let v = if has != 0 && read_ind(ind.cast_unsigned()) < 0 {
        Value::Null
    } else {
        read_value(var.cast_unsigned(), &d, extra)
    };
    let o = Out { addr: var.cast_unsigned(), desc: d, extra, ind: (has != 0).then_some(ind.cast_unsigned()), stride: 0, istride: 0 };
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        p.inputs.push(v);
        p.in_vars.push(o);
    });
}

/// `_QRNX_SQLOUT(var, desc, extra, ind, has_ind, stride)`: an output
/// host variable; `stride` is a structure array's element size.
pub extern "sysv64" fn sql_out(_env: *mut Env, _argc: i64, _od: i64, var: i64, desc: i64, extra: i64, ind: i64, has: i64, stride: i64) {
    if sqltype(extra).is_some_and(|k| k & 0xF == 3) {
        PENDING.with(|p| p.borrow_mut().file_ref = true);
    }
    let o = Out {
        addr: var.cast_unsigned(),
        desc: Desc::of(desc),
        extra,
        ind: (has != 0).then_some(ind.cast_unsigned()),
        stride,
        istride: stride,
    };
    PENDING.with(|p| p.borrow_mut().outs.push(o));
}

/// `_QRNX_SQLINA(var, desc, extra, ind, has_ind, stride, istride)`: an
/// input host variable of a blocked `INSERT`: element 1 of a structure
/// array; `stride` and `istride` the array's and its indicators'
/// element sizes.
#[allow(clippy::too_many_arguments)] // the call's parameters
pub extern "sysv64" fn sql_in_a(env: *mut Env, argc: i64, od: i64, var: i64, desc: i64, extra: i64, ind: i64, has: i64, stride: i64, istride: i64) {
    sql_in(env, argc, od, var, desc, extra, ind, has);
    let o = Out {
        addr: var.cast_unsigned(),
        desc: Desc::of(desc),
        extra,
        ind: (has != 0).then_some(ind.cast_unsigned()),
        stride,
        istride,
    };
    PENDING.with(|p| p.borrow_mut().rows_in.push(o));
}

/// `_QRNX_SQLOUTA(var, desc, extra, ind, has_ind, stride, istride)`: an
/// output host variable of a multiple-row `FETCH` whose indicators are an
/// array of their own.
#[allow(clippy::too_many_arguments)] // the call's parameters
pub extern "sysv64" fn sql_out_a(_env: *mut Env, _argc: i64, _od: i64, var: i64, desc: i64, extra: i64, ind: i64, has: i64, stride: i64, istride: i64) {
    let o = Out {
        addr: var.cast_unsigned(),
        desc: Desc::of(desc),
        extra,
        ind: (has != 0).then_some(ind.cast_unsigned()),
        stride,
        istride,
    };
    PENDING.with(|p| p.borrow_mut().outs.push(o));
}

/// The values of row `r` of a blocked `INSERT`'s inputs.
fn row_inputs(ins: &[Out], r: u64) -> Vec<Value> {
    ins.iter()
        .map(|o| {
            let at = o.addr + u64::try_from(o.stride.max(0)).unwrap_or(0) * r;
            let ind = o.ind.map(|i| i + u64::try_from(o.istride.max(0)).unwrap_or(0) * r);
            if ind.is_some_and(|i| read_ind(i) < 0) { Value::Null } else { read_value(at, &o.desc, o.extra) }
        })
        .collect()
}

/// An SQLDA's header: SQLDAID, SQLDABC, SQLN, SQLD; its entries of 80
/// bytes follow (SQLTYPE, SQLLEN, SQLRES, SQLDATA, SQLIND, SQLNAMELEN,
/// SQLNAME).
const DA_HEAD: usize = 16;
const DA_VAR: usize = 80;

fn be16(addr: u64) -> i16 {
    let b = bytes(addr, 2);
    i16::from_be_bytes([b[0], b[1]])
}

/// The storage a pointer in an SQLDA entry addresses; `None` for null.
fn da_ptr(job: &mut mi::Job, addr: u64) -> Option<u64> {
    let b: [u8; 16] = bytes(addr, 16).try_into().ok()?;
    let p = mi::space::FatPtr::from_bytes(&b);
    job.spaces.deref(p).ok()
}

/// An entry's SQLTYPE and SQLLEN as a host variable's descriptor.
fn da_desc(sqltype: i16, sqllen: i16) -> Desc {
    let len = u32::from(sqllen.cast_unsigned());
    let (kind, len, dec) = match sqltype & !1 {
        448 | 456 => (Kind::VarChar, len, 0),
        484 => (Kind::Packed, len >> 8, u8::try_from(len & 0xFF).unwrap_or(0)),
        488 => (Kind::Zoned, len >> 8, u8::try_from(len & 0xFF).unwrap_or(0)),
        496 => (Kind::Int, 10, 0),
        500 => (Kind::Int, 5, 0),
        492 => (Kind::Int, 20, 0),
        480 => (Kind::Float, len, 0),
        384 => (Kind::Date, len, 0),
        388 => (Kind::Time, len, 0),
        392 => (Kind::Timestamp, len, 0),
        _ => (Kind::Char, len, 0),
    };
    Desc { kind, len, dec }
}

/// `_QRNX_SQLDA(da, mode)`: a statement's SQLDA. Mode 1: `USING
/// DESCRIPTOR`, its entries are the inputs; 2: `FETCH ... USING
/// DESCRIPTOR`, the outputs; 3: `DESCRIBE` or `PREPARE INTO` fills it.
pub extern "sysv64" fn sql_da(env: *mut Env, _argc: i64, _od: i64, da: i64, mode: i64) {
    let da = da.cast_unsigned();
    if mode == 3 {
        PENDING.with(|p| p.borrow_mut().da = Some(da));
        return;
    }
    let job = mi::storage::job_of(env);
    let n = usize::try_from(be16(da + 14)).unwrap_or(0);
    for k in 0..n {
        let at = da + (DA_HEAD + k * DA_VAR) as u64;
        let ty = be16(at);
        let desc = da_desc(ty, be16(at + 2));
        let Some(data) = da_ptr(job, at + 16) else { continue };
        let ind = if ty & 1 == 1 { da_ptr(job, at + 32) } else { None };
        if mode == 1 {
            let v = if ind.is_some_and(|i| read_ind(i) < 0) { Value::Null } else { read_value(data, &desc, 0) };
            PENDING.with(|p| {
                let mut p = p.borrow_mut();
                p.inputs.push(v);
                p.in_vars.push(Out { addr: data, desc, extra: 0, ind, stride: 0, istride: 0 });
            });
        } else {
            let o = Out {
                addr: data,
                desc,
                extra: 0,
                ind,
                stride: 0,
                istride: 0,
            };
            PENDING.with(|p| p.borrow_mut().outs.push(o));
        }
    }
}

/// How many names each column has in the SQLDA `USING` asks for: 1, 2
/// (`BOTH`) or 3 (`ALL`).
fn name_sets(using: &str) -> usize {
    match using {
        "BOTH" => 2,
        "ALL" => 3,
        _ => 1,
    }
}

/// A row's whole number at `i`.
fn row_num(r: &[Value], i: usize) -> i64 {
    r.get(i).and_then(|x| x.as_i64().ok()).unwrap_or(0)
}

/// A row's text at `i`.
fn row_text(r: &[Value], i: usize) -> String {
    r.get(i).map(Value::as_text).unwrap_or_default().trim_end().to_string()
}

/// SQLNAMELEN and SQLNAME at offset 48 of an entry.
fn put_name(v: &mut [u8], name: &str) {
    let mut e = ebcdic(name);
    e.truncate(30);
    v[48..50].copy_from_slice(&u16::try_from(e.len()).unwrap_or(0).to_be_bytes());
    e.resize(30, BLANK);
    v[50..80].copy_from_slice(&e);
}

/// Fill an SQLDA from a `DESCRIBE`'s rows (`sql::embed::describe::rows`)
/// as IBM does (`tools/conformance/rpg/b11sql/SQL056`, `SQL057`): the
/// header always; the entries when SQLN holds every set; only the base
/// entries, with `+237`, when large objects need extended entries that
/// do not fit. `using` is the `USING` choice. The warning, if any, is
/// the SQLCODE and SQLSTATE.
fn write_da(da: u64, rows: &[Vec<Value>], using: &str) -> Option<(i32, &'static str)> {
    let sqln = usize::try_from(be16(da + 12)).unwrap_or(0);
    let lobs = rows.iter().any(|r| matches!(row_num(r, 0) & !1, 404 | 408 | 412));
    let sets = name_sets(using).max(if lobs { 2 } else { 1 });
    let head = bytes(da, DA_HEAD);
    let mut id = ebcdic("SQLDA   ");
    if sets > 1 {
        id[6] = ccsid::byte_of(char::from(b'0' + u8::try_from(sets).unwrap_or(2))).unwrap_or(BLANK);
    }
    head[..8].copy_from_slice(&id);
    let bc = u32::try_from(DA_HEAD + DA_VAR * sqln).unwrap_or(0);
    head[8..12].copy_from_slice(&bc.to_be_bytes());
    head[14..16].copy_from_slice(&i16::try_from(rows.len()).unwrap_or(i16::MAX).to_be_bytes());
    let n = rows.len();
    if sqln < n {
        return lobs.then_some((239, "01005"));
    }
    let entry = |k: usize| bytes(da + (DA_HEAD + k * DA_VAR) as u64, DA_VAR);
    for (k, r) in rows.iter().enumerate() {
        let v = entry(k);
        let ty = row_num(r, 0);
        let lob = matches!(ty & !1, 404 | 408 | 412);
        let num = |x: i64| i16::try_from(x).unwrap_or(0).to_be_bytes();
        v[..2].copy_from_slice(&num(ty));
        v[2..4].copy_from_slice(&num(if lob { 0 } else { row_num(r, 1) }));
        v[4..48].fill(0);
        v[13] = u8::try_from(row_num(r, 5)).unwrap_or(0);
        v[15] = u8::try_from(row_num(r, 6)).unwrap_or(0);
        v[18..20].copy_from_slice(&u16::try_from(row_num(r, 3)).unwrap_or(0).to_be_bytes());
        let name = match using {
            "SYSTEM" => row_text(r, 8),
            "LABELS" | "ANY" => row_text(r, 7),
            _ => row_text(r, 2),
        };
        put_name(v, &name);
    }
    if sqln < n * sets {
        return lobs.then_some((237, "01594"));
    }
    for (k, r) in rows.iter().enumerate() {
        if lobs {
            // The extended entry of a large object: its whole length.
            if matches!(row_num(r, 0) & !1, 404 | 408 | 412) {
                let v = entry(n + k);
                v[..4].copy_from_slice(&u32::try_from(row_num(r, 1)).unwrap_or(0).to_be_bytes());
                v[32..50].fill(0);
            }
            continue;
        }
        if sets >= 2 {
            put_name(entry(n + k), &row_text(r, 7));
        }
        if sets == 3 {
            put_name(entry(2 * n + k), "");
        }
    }
    None
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
    if let Some(k) = sqltype(extra) {
        return Ok(lob_image(v, d, k));
    }
    let n = d.size();
    match d.kind {
        Kind::Char | Kind::Ind | Kind::VarChar => {
            let text = match v {
                Value::Str(s) => s.clone(),
                Value::Date(x) => x.render(DateFmt::Iso, None),
                Value::Time(x) => x.render(TimeFmt::Iso, None),
                Value::Timestamp(t) => ts_text(t),
                Value::Num(x) => x.to_string(),
                Value::Double(_) | Value::Real(_) => v.to_string(),
                Value::DecFloat(x) => x.to_string(),
                // Bit data: its bytes; a boolean as an indicator holds it.
                Value::Bin(b) => ccsid::from_ebcdic(b),
                Value::Bool(b) => if *b { "1" } else { "0" }.into(),
                Value::Null => String::new(),
            };
            let e = ebcdic(&text);
            let cap = if d.kind == Kind::VarChar { d.len as usize } else { n };
            let put = if e.len() > cap && e[cap..].iter().any(|b| *b != BLANK) { Put::Truncated(e.len()) } else { Put::Ok };
            let mut body: Vec<u8> = e.into_iter().take(cap).collect();
            if d.kind == Kind::VarChar {
                let mut out = vec![0; d.prefix()];
                crate::value::set_var_len(&mut out, d.prefix(), body.len());
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
        // A float host variable takes a float as it is.
        Kind::Float => {
            let f = v.as_f64().map_err(|_| (-420, "22018"))?;
            #[allow(clippy::cast_possible_truncation)]
            Ok((if d.len == 4 { (f as f32).to_be_bytes().to_vec() } else { f.to_be_bytes().to_vec() }, Put::Ok))
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
        t.micros()
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
    if sqltype(extra).is_some() {
        return vec![0; 4];
    }
    match d.kind {
        Kind::Char | Kind::Ind => vec![BLANK; d.size()],
        Kind::VarChar => vec![0; d.prefix()],
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
        let ind = o.ind.map(|i| i + u64::try_from(o.istride.max(0)).unwrap_or(0) * r as u64);
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
    let Some(mut desc) = SqlDesc::decode(&desc_text) else {
        write_sqlca(ca.cast_unsigned(), &Sqlca::from_error(&sql::SqlError::Unsupported { what: "statement".into() }));
        return;
    };
    // This runtime makes the calls a CALL plans.
    desc.calls = true;
    if pending.file_ref {
        let e = sql::SqlError::Unsupported { what: "LOB file reference variables".into() };
        write_sqlca(ca.cast_unsigned(), &Sqlca::from_error(&e));
        return;
    }
    let job = mi::storage::job_of(env);
    // A module's statements belong to it in its activation group.
    let end = desc.op == sql::embed::Op::End;
    let using = desc.orient.split_whitespace().nth(1).unwrap_or("NAMES").to_string();
    let blocked = desc.op == sql::embed::Op::Exec && desc.rows && !pending.rows_in.is_empty();
    let mut req = Request {
        scope: format!("{}:{scope}", job.act.current),
        desc,
        inputs: pending.inputs,
    };
    let Some(session) = job.sql.as_mut() else {
        let none = Sqlca::from_error(&sql::SqlError::Database {
            what: "no database for embedded SQL in this job".into(),
            msgid: None,
        });
        if !end {
            write_sqlca(ca.cast_unsigned(), &none);
        }
        return;
    };
    // A blocked INSERT: every row's values, then the count.
    if blocked {
        let n = req.inputs.last().and_then(|v| v.as_i64().ok()).unwrap_or(0).max(0);
        let mut all: Vec<Value> = (0..n.cast_unsigned()).flat_map(|r| row_inputs(&pending.rows_in, r)).collect();
        all.push(Value::int(n));
        req.inputs = all;
    }
    let reply = session.run(&req);
    let reply = match reply.call.clone() {
        Some(plan) => call(job, &req.scope, &plan, &pending.in_vars),
        None => reply,
    };
    // The end of a module's statements leaves its SQLCA alone.
    if end {
        return;
    }
    let mut sqlca = reply.sqlca;
    if let Some(da) = pending.da {
        if !sqlca.is_error()
            && let Some((code, state)) = write_da(da, &reply.rows, &using)
        {
            sqlca.sqlcode = code;
            sqlca.sqlstate = state.into();
        }
        write_sqlca(ca.cast_unsigned(), &sqlca);
        return;
    }
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

/// A `CALL`'s plan carried out: the call, its outputs into the
/// statement's variables, then the database told.
fn call(job: &mut mi::Job, scope: &str, plan: &sql::exec::routine::CallPlan, vars: &[Out]) -> sql::embed::Reply {
    let out = crate::sqlcall::perform(job, plan);
    let mut ca = Sqlca::default();
    for (m, v) in &out.outs {
        if let Some(o) = vars.get(*m) {
            assign(std::slice::from_ref(o), std::slice::from_ref(v), 0, &mut ca);
        }
    }
    let mut called = out.called;
    if ca.is_error() && called.sqlcode >= 0 {
        called.sqlcode = ca.sqlcode;
        called.sqlstate.clone_from(&ca.sqlstate);
    }
    let req = Request {
        scope: scope.to_string(),
        desc: SqlDesc { op: sql::embed::Op::Called, ..SqlDesc::default() },
        inputs: called.inputs(),
    };
    let ok = called.sqlcode >= 0;
    let Some(s) = job.sql.as_mut() else { return sql::embed::Reply::default() };
    let r = s.run(&req);
    if ok && plan.commit_on_return {
        let commit = Request { scope: scope.to_string(), desc: SqlDesc { op: sql::embed::Op::Exec, text: "COMMIT".into(), ..SqlDesc::default() }, inputs: Vec::new() };
        let c = s.run(&commit);
        if c.sqlca.is_error() && c.sqlca.sqlcode != -7007 {
            return c;
        }
    }
    r
}

/// The bytes a value takes in storage of `d`, for a call's argument.
pub(crate) fn value_image(v: &Value, d: &Desc, extra: i64) -> Result<Vec<u8>, (i32, &'static str)> {
    image(v, d, extra).map(|(b, _)| b)
}

/// The bytes of an empty value of `d`.
pub(crate) fn empty_image(d: &Desc, extra: i64) -> Vec<u8> {
    empty(d, extra)
}

/// The value in storage of `d`.
pub(crate) fn value_of(b: &[u8], d: &Desc, extra: i64) -> Value {
    read_value(b.as_ptr() as u64, d, extra)
}

/// An SQLCA into a buffer.
pub(crate) fn write_sqlca_at(b: &mut [u8], ca: &Sqlca) {
    write_sqlca(b.as_mut_ptr() as u64, ca);
}

/// An SQLCA from a buffer in IBM's layout.
pub(crate) fn read_sqlca_at(b: &[u8]) -> Sqlca {
    let be = |at: usize| i32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    let ml = usize::from(u16::from_be_bytes([b[16], b[17]])).min(70);
    let mut ca = Sqlca {
        sqlcode: be(12),
        sqlstate: ccsid::from_ebcdic(&b[131..136]),
        sqlerrmc: ccsid::from_ebcdic(&b[18..18 + ml]).trim_end().to_string(),
        ..Sqlca::default()
    };
    for i in 0..6 {
        ca.sqlerrd[i] = be(96 + i * 4);
    }
    ca
}
