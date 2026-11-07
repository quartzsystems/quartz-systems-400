// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DATA-GEN`: the generator named by `%GEN` is called once per event
//! with IBM's `QrnDgParm_T` (`QOAR/QRPGLESRC,QRNDTAGEN`, 8480 bytes):
//! the start, each structure, array and scalar of the variable in
//! order, the end, and the terminate event when the generator asks for
//! it. The text it adds through `QrnDgEnv` becomes the output variable.
//!
//! Statuses: 352 an option of `%DATA` that is not valid, 355 a
//! generator that cannot be called, 361 data that cannot be prepared,
//! 362 a generator that called the procedures wrongly, 363 an error the
//! generator reported.

#![allow(clippy::match_same_arms, clippy::similar_names, clippy::too_many_lines)]

use std::cell::RefCell;
use std::fmt::Write as _;

use codegen::abi::Env;

use crate::datainto::{Callee, Region, UserParm, call_callee, callee, decode, escape, user_parm};

/// The parameter's length.
const PARM_LEN: usize = 8480;

/// Offsets in `QrnDgParm_T`.
mod at {
    pub const ENV: usize = 16;
    pub const HANDLE: usize = 32;
    pub const USER: usize = 64;
    pub const USER_SIZE: usize = 88;
    pub const USER_CCSID: usize = 92;
    pub const USER_TYPE: usize = 96;
    pub const TO_FILE: usize = 97;
    pub const TERMINATE: usize = 98;
    pub const SEQUENCE: usize = 99;
    pub const NAME: usize = 124;
    pub const EVENT: usize = 8320;
    pub const INFO: usize = 8368;
}

/// One node of the variable's shape (`rpgc::codegen::datagen`).
#[derive(Clone, Debug, Default)]
pub struct Node {
    name: String,
    desc: i64,
    off: u64,
    dim: u32,
    stride: u64,
    dtype: u8,
    dtz: u8,
    sep: Option<char>,
    ccsid: u32,
    children: Vec<Node>,
}

fn read_shape(recs: &[&str], i: &mut usize) -> Option<Node> {
    let f: Vec<&str> = recs.get(*i)?.split('|').collect();
    *i += 1;
    let n: usize = f.get(5)?.parse().ok()?;
    let mut node = Node {
        name: (*f.first()?).to_string(),
        desc: f.get(1)?.parse().ok()?,
        off: f.get(2)?.parse().ok()?,
        dim: f.get(3)?.parse().ok()?,
        stride: f.get(4)?.parse().ok()?,
        dtype: f.get(6)?.parse().ok()?,
        dtz: f.get(7)?.parse().ok()?,
        sep: f
            .get(8)
            .and_then(|s| s.chars().next())
            .filter(|c| *c != '~'),
        ccsid: f.get(9)?.parse().ok()?,
        children: Vec::new(),
    };
    for _ in 0..n {
        node.children.push(read_shape(recs, i)?);
    }
    Some(node)
}

/// The options of `%DATA` for `DATA-GEN`.
#[derive(Clone, Debug, Default)]
struct Options {
    name: Option<String>,
    countprefix: String,
}

fn options(text: &str) -> Result<Options, (u16, u32)> {
    let mut o = Options::default();
    for w in text.split_whitespace() {
        let Some((k, v)) = w.split_once('=') else {
            return Err((352, 1));
        };
        match k.to_ascii_lowercase().as_str() {
            "doc" if v.eq_ignore_ascii_case("string") => {}
            // Output to a stream file is not written (docs/RPG.md).
            "doc" | "fileccsid" => return Err((352, 2)),
            "output" if v.eq_ignore_ascii_case("clear") => {}
            // Sequences (*START, output=continue) belong to files.
            "output" => return Err((352, 2)),
            "name" => o.name = Some(v.to_string()),
            "countprefix" => o.countprefix = v.to_ascii_uppercase(),
            "renameprefix" => {}
            _ => return Err((352, 2)),
        }
    }
    Ok(o)
}

/// An event's information.
enum Info {
    None,
    Ds {
        elem: u32,
        total: u32,
        subs: u32,
        num: u32,
        names: Vec<String>,
    },
    Array {
        total: u32,
        subs: u32,
        num: u32,
    },
    Scalar {
        elem: u32,
        total: u32,
        num: u32,
        node: Node,
        text: String,
    },
}

/// One `DATA-GEN` in progress.
struct Run {
    out: String,
    error: Option<(u16, u32)>,
    frames: usize,
    /// The subfield names of the structure the current event is about.
    subfields: Vec<String>,
}

thread_local! {
    static RUNS: RefCell<Vec<Run>> = const { RefCell::new(Vec::new()) };
}

/// A scalar's value as the generator gets it, in UTF-16 for numbers,
/// dates and times.
fn scalar_text(n: &Node, b: &[u8]) -> String {
    use crate::value::Kind as K;
    let d = crate::value::Desc::of(n.desc);
    match d.kind {
        K::Float => {
            let v = crate::value::read_f64(b.as_ptr() as u64, &d).unwrap_or(0.0);
            crate::natives::editflt_text(v, d.len == 4)
        }
        K::Packed | K::Zoned | K::Bin | K::Int | K::Uns => {
            match crate::value::read_num(b.as_ptr() as u64, &d) {
                Ok(v) => {
                    let s = v.char_text(d.dec);
                    if let Some(r) = s.strip_prefix('.') {
                        format!("0.{r}")
                    } else if let Some(r) = s.strip_prefix("-.") {
                        format!("-0.{r}")
                    } else {
                        s
                    }
                }
                Err(_) => String::new(),
            }
        }
        K::Ind => {
            if b.first() == Some(&0xF1) {
                "1".into()
            } else {
                "0".into()
            }
        }
        K::VarChar => ccsid::from_ebcdic(d.var_data(b)),
        // Fixed characters without their trailing blanks.
        _ if n.ccsid == 13488 => decode(13488, b).trim_end().to_string(),
        _ => ccsid::from_ebcdic(b).trim_end().to_string(),
    }
}

/// The events for a node: (event, name, info).
fn events(
    n: &Node,
    base: u64,
    num: u32,
    name: &str,
    o: &Options,
    out: &mut Vec<(u32, String, Info)>,
) -> Result<(), (u16, u32)> {
    let is_counter = |c: &Node, parent: &Node| {
        !o.countprefix.is_empty()
            && c.name.to_ascii_uppercase().starts_with(&o.countprefix)
            && parent
                .children
                .iter()
                .any(|x| x.name.eq_ignore_ascii_case(&c.name[o.countprefix.len()..]))
    };
    let count_of = |parent: &Node, base: u64, c: &Node| -> Result<Option<u32>, (u16, u32)> {
        if o.countprefix.is_empty() {
            return Ok(None);
        }
        let Some(k) = parent.children.iter().find(|x| {
            x.name
                .eq_ignore_ascii_case(&format!("{}{}", o.countprefix, c.name))
        }) else {
            return Ok(None);
        };
        let d = crate::value::Desc::of(k.desc);
        let v = crate::value::read_num(base + k.off, &d).map_err(|_| (361, 1))?;
        let n = v.to_string().parse::<i64>().unwrap_or(-1);
        let max = i64::from(c.dim.max(1));
        if n < 0 || n > max {
            return Err((361, 3));
        }
        Ok(Some(u32::try_from(n).unwrap_or(0)))
    };
    let structure = |n: &Node,
                     at: u64,
                     elem: u32,
                     total: u32,
                     num: u32,
                     out: &mut Vec<(u32, String, Info)>|
     -> Result<(), (u16, u32)> {
        let kids: Vec<&Node> = n.children.iter().filter(|c| !is_counter(c, n)).collect();
        let subs = u32::try_from(kids.len()).unwrap_or(0);
        let names: Vec<String> = kids.iter().map(|c| c.name.clone()).collect();
        out.push((
            5,
            name.to_string(),
            Info::Ds {
                elem,
                total,
                subs,
                num,
                names: names.clone(),
            },
        ));
        for (k, c) in kids.iter().enumerate() {
            let shown = count_of(n, at, c)?;
            let c2 = match shown {
                Some(m) if c.dim > 0 => Node {
                    dim: m,
                    ..(*c).clone()
                },
                Some(0) => continue,
                _ => (*c).clone(),
            };
            events(
                &c2,
                at + c.off,
                u32::try_from(k + 1).unwrap_or(0),
                &c.name,
                o,
                out,
            )?;
        }
        out.push((
            6,
            name.to_string(),
            Info::Ds {
                elem,
                total,
                subs,
                num,
                names,
            },
        ));
        Ok(())
    };
    let scalar = |n: &Node, at: u64, elem: u32, total: u32| {
        let b = crate::value::bytes(at, usize::try_from(n.stride).unwrap_or(0)).to_vec();
        Info::Scalar {
            elem,
            total,
            num,
            node: n.clone(),
            text: scalar_text(n, &b),
        }
    };
    if n.children.is_empty() && n.desc != 0 || n.children.is_empty() && n.dtype != 0 {
        if n.dim == 0 {
            out.push((11, name.to_string(), scalar(n, base, 0, 0)));
            return Ok(());
        }
        out.push((
            7,
            name.to_string(),
            Info::Array {
                total: n.dim,
                subs: 0,
                num,
            },
        ));
        for k in 0..n.dim {
            out.push((
                11,
                name.to_string(),
                scalar(n, base + n.stride * u64::from(k), k + 1, n.dim),
            ));
        }
        out.push((
            8,
            name.to_string(),
            Info::Array {
                total: n.dim,
                subs: 0,
                num,
            },
        ));
        return Ok(());
    }
    if n.dim == 0 {
        return structure(n, base, 0, 0, num, out);
    }
    let subs = u32::try_from(n.children.iter().filter(|c| !is_counter(c, n)).count()).unwrap_or(0);
    out.push((
        9,
        name.to_string(),
        Info::Array {
            total: n.dim,
            subs,
            num,
        },
    ));
    for k in 0..n.dim {
        structure(n, base + n.stride * u64::from(k), k + 1, n.dim, num, out)?;
    }
    out.push((
        10,
        name.to_string(),
        Info::Array {
            total: n.dim,
            subs,
            num,
        },
    ));
    Ok(())
}

fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    put(b, at, &v.to_be_bytes());
}

/// A name as `VARUCS2(4096:4)`: the length in characters, then UTF-16.
fn put_name(b: &mut [u8], at: usize, name: &str) {
    let units: Vec<u16> = name.encode_utf16().take(4096).collect();
    put_u32(b, at, u32::try_from(units.len()).unwrap_or(0));
    for (i, u) in units.iter().enumerate() {
        put(b, at + 4 + 2 * i, &u.to_be_bytes());
    }
}

/// An event failed in a procedure the generator called: the escape goes
/// to the program that ran `DATA-GEN`.
fn fail_in_callback(env: *mut Env, status: u16, reason: u32) {
    let j = mi::storage::job_of(env);
    let frames = RUNS.with(|r| {
        let mut r = r.borrow_mut();
        let run = r.last_mut()?;
        if run.error.is_some() {
            return None;
        }
        run.error = Some((status, reason));
        Some(run.frames)
    });
    if let Some(frames) = frames {
        let up = u32::try_from(j.act.frames.len().saturating_sub(frames)).unwrap_or(0);
        let id = format!("RNX{status:04}");
        let _ = j.send_message(
            &id,
            "QRNXMSG",
            "*LIBL",
            reason.to_be_bytes().to_vec(),
            qir::MsgType::Escape,
            mi::QueueRef::Up(up),
        );
    }
}

fn add(text: &str) {
    RUNS.with(|r| {
        if let Some(run) = r.borrow_mut().last_mut()
            && run.error.is_none()
        {
            run.out.push_str(text);
        }
    });
}

fn bytes_at(j: &mi::Job, lo: u64, hi: u64, len: usize) -> Option<Vec<u8>> {
    let p = mi::FatPtr::from_halves(lo, hi);
    if p.is_null() {
        return None;
    }
    match j.spaces.deref(p) {
        Ok(a) if a != 0 => Some(crate::value::bytes(a, len).to_vec()),
        _ => None,
    }
}

fn low(v: u64) -> usize {
    usize::try_from(v & 0xFFFF_FFFF).unwrap_or(0)
}

/// `QrnDgReportError(handle, code)`.
extern "sysv64" fn cb_error(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _rc: u64) {
    fail_in_callback(env, 363, 0);
}

/// `QrnDgTrace(handle, text, nested)`: nothing is traced.
extern "sysv64" fn cb_trace(
    _env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    _m0: u64,
    _m1: u64,
    _n: u64,
) {
}

/// `QrnDgAddText(handle, text, chars)`: UTF-16.
extern "sysv64" fn cb_add(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    t0: u64,
    t1: u64,
    chars: u64,
) {
    let j = mi::storage::job_of(env);
    match bytes_at(j, t0, t1, low(chars) * 2) {
        Some(b) => add(&decode(1200, &b)),
        None => fail_in_callback(env, 362, 3),
    }
}

/// `QrnDgAddTextCcsid(handle, text, bytes, ccsid)`.
extern "sysv64" fn cb_add_ccsid(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    t0: u64,
    t1: u64,
    n: u64,
    ccsid: u64,
) {
    let j = mi::storage::job_of(env);
    let c = u32::try_from(low(ccsid)).unwrap_or(0);
    if matches!(c, 1200 | 13488) && low(n) % 2 == 1 {
        fail_in_callback(env, 362, 1);
        return;
    }
    match bytes_at(j, t0, t1, low(n)) {
        Some(b) => add(&decode(c, &b)),
        None => fail_in_callback(env, 362, 3),
    }
}

/// `QrnDgAddTextString(handle, text)`: ends at a null, the job's CCSID.
extern "sysv64" fn cb_add_string(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    t0: u64,
    t1: u64,
) {
    let j = mi::storage::job_of(env);
    let p = mi::FatPtr::from_halves(t0, t1);
    let Ok((base, room)) = j.spaces.extent(p) else {
        fail_in_callback(env, 362, 3);
        return;
    };
    let len = usize::try_from(room).unwrap_or(0);
    let b = crate::value::bytes(base, len);
    let n = b.iter().position(|x| *x == 0).unwrap_or(len);
    add(&ccsid::from_ebcdic(&b[..n]));
}

/// `QrnDgAddTextNewLine(handle)`: a carriage return and a line feed.
extern "sysv64" fn cb_new_line(_env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    add("\r\n");
}

/// `QrnDgGetSubfieldName(handle, number)`: the name, into the return
/// value (`VARUCS2(4096:4)`).
extern "sysv64" fn cb_subfield(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    ret: u64,
    _h0: u64,
    _h1: u64,
    num: u64,
) {
    let k = low(num);
    let name = RUNS.with(|r| {
        r.borrow()
            .last()
            .and_then(|run| run.subfields.get(k.wrapping_sub(1)).cloned())
    });
    let b = crate::value::bytes(ret, 4 + 2 * 4096);
    if let Some(n) = name {
        put_name(b, 0, &n);
    } else {
        put_u32(b, 0, 0);
        fail_in_callback(env, 362, 5);
    }
}

/// The procedures of `QrnDgEnv`, in its order.
fn env_procs() -> [usize; 7] {
    [
        cb_error as *const () as usize,
        cb_trace as *const () as usize,
        cb_add as *const () as usize,
        cb_add_ccsid as *const () as usize,
        cb_add_string as *const () as usize,
        cb_new_line as *const () as usize,
        cb_subfield as *const () as usize,
    ]
}

/// Generate: the text, or the failure (sent already when it came from a
/// procedure the generator called).
fn generate(
    j: &mut mi::Job,
    c: &Callee,
    shape: &Node,
    base: u64,
    o: &Options,
    up: &UserParm,
) -> Result<String, Option<(u16, u32)>> {
    let top = o.name.clone().unwrap_or_else(|| shape.name.clone());
    let mut evs = vec![(3, String::new(), Info::None)];
    events(shape, base, 0, &top, o, &mut evs).map_err(Some)?;
    evs.push((4, String::new(), Info::None));
    let mut envb = vec![0u8; 20 * 16];
    for (i, a) in env_procs().iter().enumerate() {
        put(
            &mut envb,
            i * 16,
            &mi::FatPtr::procedure(*a as u64).to_bytes(),
        );
    }
    let (env_r, env_p) = Region::new(j, envb);
    let (h_r, h_p) = Region::new(j, vec![0; 16]);
    let mut up_r = None;
    let (up_p, size, kind) = match up {
        UserParm::None => (mi::FatPtr::null(), 0, b'0'),
        UserParm::Text(t) => {
            let mut b = crate::value::ebcdic(t);
            let n = b.len();
            b.push(0);
            let (r, p) = Region::new(j, b);
            up_r = Some(r);
            (p, n, b'1')
        }
        UserParm::Var(a, n, ty) => {
            let depth = j.depth;
            (j.spaces.addr(*a, *n as u64, depth, false), *n, *ty)
        }
    };
    let mut pb = vec![0u8; PARM_LEN];
    put(&mut pb, at::ENV, &env_p.to_bytes());
    put(&mut pb, at::HANDLE, &h_p.to_bytes());
    put(&mut pb, at::USER, &up_p.to_bytes());
    put_u32(&mut pb, at::USER_SIZE, u32::try_from(size).unwrap_or(0));
    put_u32(&mut pb, at::USER_CCSID, 0);
    pb[at::USER_TYPE] = crate::value::ebcdic(&char::from(kind).to_string())[0];
    pb[at::TO_FILE] = 0xF0;
    pb[at::TERMINATE] = 0xF0;
    pb[at::SEQUENCE] = 0xF0;
    let (mut parm, _) = Region::new(j, pb);
    RUNS.with(|r| {
        r.borrow_mut().push(Run {
            out: String::new(),
            error: None,
            frames: j.act.frames.len(),
            subfields: Vec::new(),
        });
    });
    let mut result: Result<(), Option<(u16, u32)>> = Ok(());
    let mut terminate = false;
    for (event, name, info) in &evs {
        let b = parm.buf.as_mut_slice();
        b[at::NAME..at::EVENT].fill(0);
        put_name(b, at::NAME, name);
        put_u32(b, at::EVENT, *event);
        b[at::INFO..PARM_LEN].fill(0);
        let mut value_r = None;
        match info {
            Info::None => {}
            Info::Ds {
                elem,
                total,
                subs,
                num,
                ..
            } => {
                put_u32(b, at::INFO, *elem);
                put_u32(b, at::INFO + 4, *total);
                put_u32(b, at::INFO + 8, *subs);
                put_u32(b, at::INFO + 12, *num);
                b[at::INFO + 16] = 0xF0;
                b[at::INFO + 17..at::INFO + 60].fill(0x40);
            }
            Info::Array { total, subs, num } => {
                put_u32(b, at::INFO, *total);
                put_u32(b, at::INFO + 4, *subs);
                put_u32(b, at::INFO + 8, *num);
            }
            Info::Scalar {
                elem,
                total,
                num,
                node,
                text,
            } => {
                put_u32(b, at::INFO, *elem);
                put_u32(b, at::INFO + 4, *total);
                put_u32(
                    b,
                    at::INFO + 8,
                    if node.dtype == 1 { 37 } else { node.ccsid },
                );
                put_u32(b, at::INFO + 12, *num);
                b[at::INFO + 16] = node.dtype;
                b[at::INFO + 17] = node.dtz;
                b[at::INFO + 18] = node
                    .sep
                    .map_or(0, |c| crate::value::ebcdic(&c.to_string())[0]);
                // Every value goes in UTF-16.
                let data: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
                let (ccsid, chars) = (1200u32, data.len() / 2);
                put_u32(b, at::INFO + 44, u32::try_from(data.len()).unwrap_or(0));
                put_u32(b, at::INFO + 48, u32::try_from(chars).unwrap_or(0));
                put_u32(b, at::INFO + 52, ccsid);
                let (r, p) = Region::new(j, data);
                let b = parm.buf.as_mut_slice();
                put(b, at::INFO + 64, &p.to_bytes());
                value_r = Some(r);
            }
        }
        // The subfield names a structure event is about.
        let names = match info {
            Info::Ds { names, .. } => names.clone(),
            _ => Vec::new(),
        };
        RUNS.with(|r| {
            if let Some(run) = r.borrow_mut().last_mut() {
                run.subfields = names;
            }
        });
        let addr = parm.buf.as_ptr() as u64;
        let called = call_callee(j, c, addr);
        if let Some(r) = value_r {
            r.drop_in(j);
        }
        if !called {
            j.clear();
            result = Err(Some((355, 1)));
            break;
        }
        if RUNS.with(|r| r.borrow().last().is_some_and(|run| run.error.is_some())) || j.pending() {
            result = Err(None);
            break;
        }
        terminate = parm.buf[at::TERMINATE] == 0xF1;
    }
    if result.is_ok() && terminate {
        // The terminate event: its text is not kept.
        let saved = RUNS.with(|r| r.borrow().last().map(|run| run.out.clone()));
        let b = parm.buf.as_mut_slice();
        b[at::NAME..PARM_LEN].fill(0);
        put_name(b, at::NAME, "");
        put_u32(b, at::EVENT, 12);
        let addr = parm.buf.as_ptr() as u64;
        let _ = call_callee(j, c, addr);
        RUNS.with(|r| {
            if let (Some(run), Some(s)) = (r.borrow_mut().last_mut(), saved) {
                run.out = s;
            }
        });
    }
    let run = RUNS.with(|r| r.borrow_mut().pop());
    for r in [Some(env_r), Some(h_r), Some(parm), up_r]
        .into_iter()
        .flatten()
    {
        r.drop_in(j);
    }
    result?;
    Ok(run.map(|r| r.out).unwrap_or_default())
}

/// `_QRNX_DATAGEN(var, shape, out, od, opts, optd, name, nd, ptr, up,
/// ud, uk)`: generate the variable's document into `out`.
pub extern "sysv64" fn data_gen(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    var: i64,
    shp: i64,
    out: i64,
    od: i64,
    opts: i64,
    optd: i64,
    name: i64,
    nd: i64,
    ptr: i64,
    up: i64,
    ud: i64,
    uk: i32,
) {
    let j = mi::storage::job_of(env);
    let text = crate::xml::varchar_at(shp);
    let recs: Vec<&str> = text.split(';').filter(|r| !r.is_empty()).collect();
    let mut i = 0;
    let Some(shape) = read_shape(&recs, &mut i) else {
        return;
    };
    let o = match options(&crate::xml::text_of(opts, optd)) {
        Ok(o) => o,
        Err((s, r)) => {
            escape(j, s, r);
            return;
        }
    };
    let c = match crate::xml::proc_at(ptr) {
        Some(a) => Callee::Ptr(a),
        None => callee(&crate::xml::text_of(name, nd)),
    };
    let user = user_parm(up, ud, uk);
    match generate(j, &c, &shape, var.cast_unsigned(), &o, &user) {
        Ok(doc) => {
            let d = crate::value::Desc::of(od);
            let e = crate::value::ebcdic(&doc);
            if d.kind == crate::value::Kind::VarChar {
                let mut e = e;
                e.truncate(usize::try_from(d.len).unwrap_or(0));
                crate::value::write_var(out.cast_unsigned(), d.prefix(), &e);
            } else {
                let b = crate::value::bytes(out.cast_unsigned(), d.size());
                b.fill(0x40);
                let n = e.len().min(b.len());
                b[..n].copy_from_slice(&e[..n]);
            }
        }
        Err(Some((s, r))) => escape(j, s, r),
        Err(None) => {}
    }
}

/// The document a generator QS/400 ships writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// JSON: structures as objects, arrays as arrays, numbers bare,
    /// indicators as `true` and `false`.
    Json,
    /// XML: each structure and scalar an element named after it, an
    /// array's elements one after another under its name.
    Xml,
}

thread_local! {
    /// The open structures and arrays of a shipped generator: whether
    /// an item was written in each, and whether it is an array.
    static GEN: RefCell<Vec<(bool, bool)>> = const { RefCell::new(Vec::new()) };
}

fn json_string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// One event of a shipped generator.
pub fn shipped_generator(j: &mut mi::Job, args: &[u64], style: Style) {
    let Some(&parm) = args.first() else { return };
    let b = crate::value::bytes(parm, PARM_LEN).to_vec();
    let event = u32::from_be_bytes([
        b[at::EVENT],
        b[at::EVENT + 1],
        b[at::EVENT + 2],
        b[at::EVENT + 3],
    ]);
    let n = u32::from_be_bytes([
        b[at::NAME],
        b[at::NAME + 1],
        b[at::NAME + 2],
        b[at::NAME + 3],
    ]) as usize;
    let name = decode(1200, &b[at::NAME + 4..at::NAME + 4 + 2 * n.min(4096)]);
    let value = || -> (u8, String) {
        let dtype = b[at::INFO + 16];
        let len = u32::from_be_bytes([
            b[at::INFO + 44],
            b[at::INFO + 45],
            b[at::INFO + 46],
            b[at::INFO + 47],
        ]) as usize;
        let p = mi::FatPtr::from_bytes(
            &b[at::INFO + 64..at::INFO + 80]
                .try_into()
                .unwrap_or([0; 16]),
        );
        let text = match j.spaces.deref(p) {
            Ok(a) if a != 0 => decode(1200, crate::value::bytes(a, len)),
            _ => String::new(),
        };
        (dtype, text)
    };
    let mut out = String::new();
    GEN.with(|g| {
        let mut g = g.borrow_mut();
        // The separator and the name an item takes in its container.
        let lead = |g: &mut Vec<(bool, bool)>, out: &mut String| {
            if let Some((written, array)) = g.last_mut() {
                if *written && style == Style::Json {
                    out.push(',');
                }
                *written = true;
                if !*array && style == Style::Json {
                    out.push_str(&json_string(&name));
                    out.push(':');
                }
            }
        };
        match (event, style) {
            (3, _) => g.clear(),
            (5, Style::Json) => {
                lead(&mut g, &mut out);
                out.push('{');
                g.push((false, false));
            }
            (5, Style::Xml) => {
                let _ = write!(out, "<{name}>");
                g.push((false, false));
            }
            (6, Style::Json) => {
                g.pop();
                out.push('}');
            }
            (6, Style::Xml) => {
                g.pop();
                let _ = write!(out, "</{name}>");
            }
            (7 | 9, Style::Json) => {
                lead(&mut g, &mut out);
                out.push('[');
                g.push((false, true));
            }
            (8 | 10, Style::Json) => {
                g.pop();
                out.push(']');
            }
            (11, Style::Json) => {
                lead(&mut g, &mut out);
                let (dtype, text) = value();
                match dtype {
                    8..=11 => out.push_str(&text),
                    7 => out.push_str(if text == "1" { "true" } else { "false" }),
                    _ => out.push_str(&json_string(&text)),
                }
            }
            (11, Style::Xml) => {
                let (_, text) = value();
                let _ = write!(out, "<{name}>{}</{name}>", xml_text(&text));
            }
            _ => {}
        }
    });
    if !out.is_empty() {
        add(&out);
    }
}
