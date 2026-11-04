// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DATA-INTO`: the parser named by `%PARSER` is called with IBM's
//! `QrnDiParm_T` (`QOAR/QRPGLESRC,QRNDTAINTO`) and reports the
//! document through the procedures of `QrnDiEnv`; the names, values,
//! structures and arrays it reports make a tree that fills the target
//! as `XML-INTO` fills one (`qrpgrt::xml`), or goes to a `%HANDLER` in
//! groups of its array's size.
//!
//! Statuses: 352 an option of `%DATA` that is not valid, 355 a parser
//! that cannot be called, 356 a document that does not fit the target,
//! 357 an error the parser reported, 358 a parser that called the
//! procedures out of order.

#![allow(clippy::match_same_arms, clippy::similar_names, clippy::too_many_lines, clippy::struct_excessive_bools, clippy::unnecessary_wraps, clippy::unused_self)]

use std::cell::RefCell;

use codegen::abi::Env;

use crate::xml::{Shape, name_matches, store_scalar, trim_all};

/// A part of the document the parser reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A scalar value.
    Value(String),
    /// A structure: its named items (attributes among them), in order.
    Struct(Vec<(String, Node)>),
    /// An array: its items.
    Array(Vec<Node>),
}

/// An open structure or array.
#[derive(Debug)]
enum Open {
    Struct(String, Vec<(String, Node)>),
    Array(String, Vec<Node>),
}

/// Why an operation failed: the status and the message's reason code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fail {
    /// RPG status.
    pub status: u16,
    /// Reason code.
    pub reason: u32,
}

const fn fail(status: u16, reason: u32) -> Fail {
    Fail { status, reason }
}

/// The tree a parse builds from the reported events.
#[derive(Debug, Default)]
pub struct Builder {
    started: bool,
    finished: bool,
    stack: Vec<Open>,
    pending: Option<String>,
    /// The items reported outside any structure or array.
    top: Vec<(String, Node)>,
    /// The first error; later calls are ignored.
    pub error: Option<Fail>,
    /// The error code and offset the parser reported.
    pub parser_error: Option<(i32, u32)>,
}

impl Builder {
    fn misuse(&mut self, reason: u32) -> Result<(), Fail> {
        Err(fail(358, reason))
    }

    fn check(&mut self) -> Result<(), Fail> {
        if !self.started {
            return self.misuse(8);
        }
        if self.finished {
            return self.misuse(11);
        }
        Ok(())
    }

    /// `QrnDiStart`.
    pub fn start(&mut self) -> Result<(), Fail> {
        if self.started {
            return self.misuse(7);
        }
        self.started = true;
        Ok(())
    }

    /// `QrnDiFinish`.
    pub fn finish(&mut self) -> Result<(), Fail> {
        self.check()?;
        self.finished = true;
        Ok(())
    }

    /// `QrnDiReportName`.
    pub fn name(&mut self, n: &str) -> Result<(), Fail> {
        self.check()?;
        if n.is_empty() {
            return self.misuse(12);
        }
        if self.pending.is_some() || matches!(self.stack.last(), Some(Open::Array(..))) {
            return self.misuse(9);
        }
        self.pending = Some(n.to_string());
        Ok(())
    }

    /// The name the next item takes: the one reported, or none at the
    /// start and inside an array.
    fn take_name(&mut self) -> Result<String, Fail> {
        if let Some(n) = self.pending.take() {
            return Ok(n);
        }
        match self.stack.last() {
            Some(Open::Array(..)) => Ok(String::new()),
            None if self.top.is_empty() => Ok(String::new()),
            _ => Err(fail(358, 6)),
        }
    }

    fn place(&mut self, name: String, node: Node) -> Result<(), Fail> {
        match self.stack.last_mut() {
            Some(Open::Struct(_, items)) => items.push((name, node)),
            Some(Open::Array(_, items)) => items.push(node),
            None => self.top.push((name, node)),
        }
        Ok(())
    }

    /// `QrnDiReportValue`.
    pub fn value(&mut self, v: &str) -> Result<(), Fail> {
        self.check()?;
        let n = self.take_name()?;
        self.place(n, Node::Value(v.to_string()))
    }

    /// `QrnDiReportAttr`: an item of the structure being reported.
    pub fn attr(&mut self, n: &str, v: &str) -> Result<(), Fail> {
        self.check()?;
        if n.is_empty() {
            return self.misuse(12);
        }
        match self.stack.last_mut() {
            Some(Open::Struct(_, items)) if self.pending.is_none() => {
                items.push((n.to_string(), Node::Value(v.to_string())));
                Ok(())
            }
            _ => self.misuse(10),
        }
    }

    /// `QrnDiStartStruct`.
    pub fn start_struct(&mut self) -> Result<(), Fail> {
        self.check()?;
        let n = self.take_name()?;
        self.stack.push(Open::Struct(n, Vec::new()));
        Ok(())
    }

    /// `QrnDiStartArray`.
    pub fn start_array(&mut self) -> Result<(), Fail> {
        self.check()?;
        let n = self.take_name()?;
        self.stack.push(Open::Array(n, Vec::new()));
        Ok(())
    }

    /// `QrnDiEndStruct`.
    pub fn end_struct(&mut self) -> Result<(), Fail> {
        self.check()?;
        match self.stack.pop() {
            Some(Open::Struct(n, items)) => self.place(n, Node::Struct(items)),
            Some(Open::Array(..)) => self.misuse(4),
            None => self.misuse(3),
        }
    }

    /// `QrnDiEndArray`.
    pub fn end_array(&mut self) -> Result<(), Fail> {
        self.check()?;
        match self.stack.pop() {
            Some(Open::Array(n, items)) => self.place(n, Node::Array(items)),
            Some(Open::Struct(..)) => self.misuse(5),
            None => self.misuse(3),
        }
    }

    /// `QrnDiReportError`.
    pub fn report_error(&mut self, rc: i32, at: u32) -> Result<(), Fail> {
        self.check()?;
        if rc <= 0 {
            return self.misuse(2);
        }
        self.parser_error = Some((rc, at));
        Err(fail(357, 0))
    }

    /// The document's root: its name (empty when it has none) and node,
    /// the first item reported outside any structure (later ones are
    /// not used, as on IBM i).
    #[must_use]
    pub fn root(&self) -> Option<(String, Node)> {
        self.top.first().cloned()
    }
}

/// The options of `%DATA` for `DATA-INTO`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `path=`.
    pub path: Vec<String>,
    /// `case=`: lower, upper, any, convert.
    pub case: String,
    /// `allowmissing=yes`.
    pub allow_missing: bool,
    /// `allowextra=yes`.
    pub allow_extra: bool,
    /// `trim=none`.
    pub trim_none: bool,
    /// `ccsid=job`: the document goes to the parser in the job's CCSID.
    pub job_ccsid: bool,
    /// `countprefix=`.
    pub countprefix: String,
}

/// Read `%DATA`'s options: the first one not valid fails with 352.
pub fn options(text: &str) -> Result<Options, Fail> {
    let mut o = Options { case: "lower".into(), ..Options::default() };
    for w in text.split_whitespace() {
        let Some((k, v)) = w.split_once('=') else { return Err(fail(352, 1)) };
        let lv = v.to_ascii_lowercase();
        let yes_no = |s: &str| matches!(s, "yes" | "no");
        match k.to_ascii_lowercase().as_str() {
            "doc" if lv == "string" => {}
            // Documents in stream files are not read (docs/RPG.md).
            "doc" if lv == "file" => return Err(fail(352, 2)),
            "doc" => return Err(fail(352, 3)),
            "path" => o.path = v.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            "case" if matches!(lv.as_str(), "lower" | "upper" | "any" | "convert") => o.case = lv,
            "allowmissing" if yes_no(&lv) => o.allow_missing = lv == "yes",
            "allowextra" if yes_no(&lv) => o.allow_extra = lv == "yes",
            "trim" if matches!(lv.as_str(), "all" | "none") => o.trim_none = lv == "none",
            "ccsid" if matches!(lv.as_str(), "job" | "ucs2") => o.job_ccsid = lv == "job",
            "countprefix" => o.countprefix = v.to_ascii_uppercase(),
            "case" | "allowmissing" | "allowextra" | "trim" | "ccsid" => return Err(fail(352, 3)),
            _ => return Err(fail(352, 2)),
        }
    }
    Ok(o)
}

/// Whether a document's name matches an RPG name under `case=`.
fn matches(doc: &str, rpg: &str, o: &Options) -> bool {
    if o.case == "convert" {
        let conv: String = doc.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        return conv.eq_ignore_ascii_case(rpg);
    }
    name_matches(doc, rpg, &o.case)
}

/// The node the target takes: the root, or the item `path=` names (the
/// first part of the path names the root; a root without a name matches
/// any variable but no path).
fn select<'a>(root: &'a (String, Node), s: &Shape, o: &Options) -> Result<&'a Node, Fail> {
    let (name, node) = root;
    if o.path.is_empty() {
        if name.is_empty() || matches(name, &s.name, o) {
            return Ok(node);
        }
        return Err(fail(356, 1));
    }
    let eq = |x: &str, p: &str| if o.case == "any" || o.case == "convert" { x.eq_ignore_ascii_case(p) } else { x == p };
    // A path starts at a root that has a name.
    if name.is_empty() || !eq(name, &o.path[0]) {
        return Err(fail(356, 1));
    }
    let mut at = node;
    for p in &o.path[1..] {
        let Node::Struct(items) = at else { return Err(fail(356, 1)) };
        at = items.iter().find(|(n, _)| eq(n, p)).map(|(_, n)| n).ok_or(fail(356, 1))?;
    }
    Ok(at)
}

/// A scalar's text under `trim=`.
fn scalar_text(v: &str, s: &Shape, o: &Options) -> String {
    let d = crate::value::Desc::of(s.desc);
    if o.trim_none && matches!(d.kind, crate::value::Kind::Char | crate::value::Kind::VarChar) {
        v.to_string()
    } else if matches!(d.kind, crate::value::Kind::Char | crate::value::Kind::VarChar) {
        trim_all(v)
    } else {
        v.trim().to_string()
    }
}

/// Fill one element of a shape from a node.
fn fill(addr: u64, s: &Shape, node: &Node, o: &Options) -> Result<(), Fail> {
    if s.children.is_empty() {
        let Node::Value(v) = node else { return Err(fail(356, 7)) };
        let d = crate::value::Desc::of(s.desc);
        return store_scalar(addr, &d, &scalar_text(v, s, o)).map_err(|_| fail(356, 8));
    }
    let Node::Struct(items) = node else { return Err(fail(356, 14)) };
    let counter = |c: &Shape| -> Option<&Shape> {
        (!o.countprefix.is_empty()).then(|| s.children.iter().find(|x| x.name == format!("{}{}", o.countprefix, c.name))).flatten()
    };
    let is_counter = |c: &Shape| !o.countprefix.is_empty() && c.name.starts_with(&o.countprefix) && s.children.iter().any(|x| x.name == c.name[o.countprefix.len()..]);
    // Every item names a subfield, unless extra ones are allowed.
    if !o.allow_extra {
        for (n, _) in items {
            if !s.children.iter().any(|c| matches(n, &c.name, o) && !is_counter(c)) {
                return Err(fail(356, 5));
            }
        }
    }
    for c in &s.children {
        if is_counter(c) {
            continue;
        }
        let base = addr + c.off;
        let found: Vec<&Node> = items.iter().filter(|(n, _)| matches(n, &c.name, o)).map(|(_, n)| n).collect();
        if c.dim > 0 {
            // An array subfield: the items of the array reported, or the
            // items repeated under its name.
            let elems: Vec<&Node> = match found.as_slice() {
                [Node::Array(v)] => v.iter().collect(),
                other => other.to_vec(),
            };
            if elems.len() > c.dim as usize {
                return Err(fail(356, 3));
            }
            if elems.len() < c.dim as usize && !o.allow_missing && counter(c).is_none() {
                return Err(fail(356, 2));
            }
            let one = Shape { dim: 0, ..c.clone() };
            for (k, e) in elems.iter().enumerate() {
                fill(base + c.stride * k as u64, &one, e, o)?;
            }
            if let Some(n) = counter(c) {
                store_count(addr + n.off, n, elems.len())?;
            }
            continue;
        }
        match found.first() {
            Some(e) => fill(base, c, e, o)?,
            None if o.allow_missing || counter(c).is_some() => {}
            None => return Err(fail(356, 4)),
        }
        if let Some(n) = counter(c) {
            store_count(addr + n.off, n, found.len().min(1))?;
        }
    }
    Ok(())
}

fn store_count(addr: u64, s: &Shape, n: usize) -> Result<(), Fail> {
    let d = crate::value::Desc::of(s.desc);
    store_scalar(addr, &d, &n.to_string()).map_err(|_| fail(356, 8))
}

/// Fill a target from the tree: the number of elements set (0 for a
/// target that is no array).
pub fn into(addr: u64, s: &Shape, root: &(String, Node), o: &Options) -> Result<i64, Fail> {
    let node = select(root, s, o)?;
    if s.dim > 0 {
        let Node::Array(items) = node else { return Err(fail(356, 15)) };
        let one = Shape { dim: 0, ..s.clone() };
        // Elements past the array's end are not taken.
        let n = items.len().min(s.dim as usize);
        for (k, e) in items.iter().take(n).enumerate() {
            fill(addr + s.stride * k as u64, &one, e, o)?;
        }
        return Ok(i64::try_from(n).unwrap_or(0));
    }
    fill(addr, s, node, o)?;
    Ok(0)
}

/// The items a `%HANDLER` gets: those of the array selected.
fn handler_items<'a>(root: &'a (String, Node), s: &Shape, o: &Options) -> Result<Vec<&'a Node>, Fail> {
    let named = Shape { name: o.path.last().map(|p| p.to_ascii_uppercase()).unwrap_or_default(), ..s.clone() };
    let o2 = Options { case: if o.path.is_empty() { "any".into() } else { o.case.clone() }, ..o.clone() };
    let node = if o.path.is_empty() { &root.1 } else { select(root, &named, &o2)? };
    match node {
        Node::Array(v) => Ok(v.iter().collect()),
        other => Ok(vec![other]),
    }
}

/// The least storage behind the document's pointer: parsers base
/// fields longer than the document on it.
pub(crate) const DOC_ROOM: usize = 1 << 20;

/// One `DATA-INTO` in progress: the tree and where its caller is.
struct Run {
    b: Builder,
    /// The caller's frames when the parser was called.
    frames: usize,
    /// The document's CCSID as the parser has it.
    data_ccsid: u32,
}

thread_local! {
    static RUNS: RefCell<Vec<Run>> = const { RefCell::new(Vec::new()) };
}

/// Text in a CCSID: 0 the job's, 1200 or 13488 UTF-16, 1208 UTF-8.
pub(crate) fn decode(ccsid: u32, b: &[u8]) -> String {
    match ccsid {
        0 | 37 => ccsid::from_ebcdic(b),
        1200 | 13488 => {
            let u: Vec<u16> = b.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
            String::from_utf16_lossy(&u)
        }
        1208 => String::from_utf8_lossy(b).into_owned(),
        other => u16::try_from(other)
            .ok()
            .and_then(|c| ccsid::conv::to_string(c, b).ok())
            .unwrap_or_else(|| ccsid::from_ebcdic(b)),
    }
}

/// The bytes a fat pointer addresses.
fn at(j: &mi::Job, lo: u64, hi: u64, len: u64) -> Vec<u8> {
    let p = mi::FatPtr::from_halves(lo, hi);
    match j.spaces.deref(p) {
        Ok(a) if a != 0 && len > 0 => crate::value::bytes(a, usize::try_from(len).unwrap_or(0)).to_vec(),
        _ => Vec::new(),
    }
}

/// Apply one event to the innermost run; an error ends the parse: the
/// escape goes to the program that ran `DATA-INTO`, so the parser does
/// not get control back.
fn event(env: *mut Env, f: impl FnOnce(&mut Builder, u32) -> Result<(), Fail>) {
    let j = mi::storage::job_of(env);
    let failed = RUNS.with(|r| {
        let mut r = r.borrow_mut();
        let run = r.last_mut()?;
        if run.b.error.is_some() {
            return None;
        }
        let ccsid = run.data_ccsid;
        match f(&mut run.b, ccsid) {
            Ok(()) => None,
            Err(e) => {
                run.b.error = Some(e);
                Some((e, run.frames, run.b.parser_error))
            }
        }
    });
    if let Some((e, frames, perr)) = failed {
        let up = u32::try_from(j.act.frames.len().saturating_sub(frames)).unwrap_or(0);
        let data = match (e.status, perr) {
            (357, Some((rc, off))) => [rc.to_be_bytes().to_vec(), vec![0, 2], ccsid::to_ebcdic("*N").unwrap_or_default(), off.to_be_bytes().to_vec()].concat(),
            _ => e.reason.to_be_bytes().to_vec(),
        };
        let id = format!("RNX{:04}", e.status);
        let _ = j.send_message(&id, "QRNXMSG", "*LIBL", data, qir::MsgType::Escape, mi::QueueRef::Up(up));
    }
}

/// Let a parser QS/400 ships work on the innermost run's tree; its
/// failure is the operation's, as a parser's call would make it.
pub(crate) fn with_builder(j: &mut mi::Job, f: impl FnOnce(&mut Builder) -> Result<(), Fail>) {
    let failed = RUNS.with(|r| {
        let mut r = r.borrow_mut();
        let run = r.last_mut()?;
        match f(&mut run.b) {
            Ok(()) => None,
            Err(e) => {
                if e.status == 357 {
                    run.b.parser_error = Some((1, e.reason));
                }
                run.b.error = Some(e);
                Some(e)
            }
        }
    });
    if let Some(e) = failed {
        let id = format!("RNX{:04}", e.status);
        let _ = j.send_message(&id, "QRNXMSG", "*LIBL", e.reason.to_be_bytes().to_vec(), qir::MsgType::Escape, mi::QueueRef::Same);
    }
}

/// The user parameter string a parser's parameter holds, if any.
pub(crate) fn parser_options(j: &mi::Job, parm: u64) -> String {
    let b = crate::value::bytes(parm, 144);
    if b[72] != 0xF1 {
        return String::new();
    }
    let p = mi::FatPtr::from_bytes(&b[48..64].try_into().unwrap_or([0; 16]));
    let Ok((a, room)) = j.spaces.extent(p) else { return String::new() };
    let t = crate::value::bytes(a, usize::try_from(room).unwrap_or(0));
    let n = t.iter().position(|x| *x == 0).unwrap_or(t.len());
    ccsid::from_ebcdic(&t[..n])
}

fn halves_len(v: u64) -> u32 {
    u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0)
}

/// `QrnDiStart(handle)`.
extern "sysv64" fn cb_start(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.start());
}

/// `QrnDiFinish(handle)`.
extern "sysv64" fn cb_finish(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.finish());
}

/// `QrnDiReportError(handle, code, bytes)`.
extern "sysv64" fn cb_error(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, rc: u64, bytes: u64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let rc = rc as u32 as i32;
    event(env, |b, _| b.report_error(rc, halves_len(bytes)));
}

/// `QrnDiReportName(handle, name, length)`: the pointer's halves on the
/// stack after an unused register.
extern "sysv64" fn cb_name(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _r9: u64, n0: u64, n1: u64, len: u64) {
    let j = mi::storage::job_of(env);
    let raw = at(j, n0, n1, u64::from(halves_len(len)));
    event(env, |b, c| b.name(&decode(c, &raw)));
}

/// `QrnDiReportNameCcsid(handle, name, length, ccsid)`.
extern "sysv64" fn cb_name_ccsid(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _r9: u64, n0: u64, n1: u64, len: u64, ccsid: u64) {
    let j = mi::storage::job_of(env);
    let raw = at(j, n0, n1, u64::from(halves_len(len)));
    let c = halves_len(ccsid);
    event(env, |b, _| b.name(&decode(c, &raw)));
}

/// `QrnDiReportValue(handle, value, length)`.
extern "sysv64" fn cb_value(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _r9: u64, v0: u64, v1: u64, len: u64) {
    let j = mi::storage::job_of(env);
    let raw = at(j, v0, v1, u64::from(halves_len(len)));
    event(env, |b, c| {
        if matches!(c, 1200 | 13488) && raw.len() % 2 == 1 {
            return Err(fail(358, 14));
        }
        b.value(&decode(c, &raw))
    });
}

/// `QrnDiReportValueCcsid(handle, value, length, ccsid)`.
extern "sysv64" fn cb_value_ccsid(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _r9: u64, v0: u64, v1: u64, len: u64, ccsid: u64) {
    let j = mi::storage::job_of(env);
    let raw = at(j, v0, v1, u64::from(halves_len(len)));
    let c = halves_len(ccsid);
    event(env, |b, _| {
        if matches!(c, 1200 | 13488) && raw.len() % 2 == 1 {
            return Err(fail(358, 14));
        }
        b.value(&decode(c, &raw))
    });
}

extern "sysv64" fn cb_start_struct(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.start_struct());
}

extern "sysv64" fn cb_end_struct(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.end_struct());
}

extern "sysv64" fn cb_start_array(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.start_array());
}

extern "sysv64" fn cb_end_array(env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64) {
    event(env, |b, _| b.end_array());
}

/// `QrnDiTrace(handle, text, nested)`: nothing is traced.
extern "sysv64" fn cb_trace(_env: *mut Env, _argc: i64, _od: i64, _h0: u64, _h1: u64, _r9: u64, _m0: u64, _m1: u64, _nested: u64) {}

/// `QrnDiReportAttr(handle, name, length, value, length)`: the second
/// pointer is aligned to 16 on the stack.
extern "sysv64" fn cb_attr(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    n0: u64,
    n1: u64,
    nlen: u64,
    _pad: u64,
    v0: u64,
    v1: u64,
    vlen: u64,
) {
    let j = mi::storage::job_of(env);
    let n = at(j, n0, n1, u64::from(halves_len(nlen)));
    let v = at(j, v0, v1, u64::from(halves_len(vlen)));
    event(env, |b, c| b.attr(&decode(c, &n), &decode(c, &v)));
}

/// `QrnDiReportAttrCcsid(handle, name, length, ccsid, value, length,
/// ccsid)`.
extern "sysv64" fn cb_attr_ccsid(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    _h0: u64,
    _h1: u64,
    _r9: u64,
    n0: u64,
    n1: u64,
    nlen: u64,
    nccsid: u64,
    v0: u64,
    v1: u64,
    vlen: u64,
    vccsid: u64,
) {
    let j = mi::storage::job_of(env);
    let n = at(j, n0, n1, u64::from(halves_len(nlen)));
    let v = at(j, v0, v1, u64::from(halves_len(vlen)));
    let (nc, vc) = (halves_len(nccsid), halves_len(vccsid));
    event(env, |b, _| b.attr(&decode(nc, &n), &decode(vc, &v)));
}

/// The procedures of `QrnDiEnv`, in its order.
fn env_procs() -> [usize; 14] {
    [
        cb_start as *const () as usize,
        cb_finish as *const () as usize,
        cb_error as *const () as usize,
        cb_name as *const () as usize,
        cb_value as *const () as usize,
        cb_start_struct as *const () as usize,
        cb_end_struct as *const () as usize,
        cb_start_array as *const () as usize,
        cb_end_array as *const () as usize,
        cb_name_ccsid as *const () as usize,
        cb_value_ccsid as *const () as usize,
        cb_trace as *const () as usize,
        cb_attr as *const () as usize,
        cb_attr_ccsid as *const () as usize,
    ]
}

/// A parser or generator `%PARSER` or `%GEN` names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Callee {
    /// A program, `LIB/PGM` or `PGM`.
    Pgm(String, String),
    /// A procedure of a service program, `LIB/SRVPGM(PROC)`.
    Proc(String, String, String),
    /// A procedure pointer's code.
    Ptr(usize),
}

/// The callee a name gives.
#[must_use]
pub fn callee(name: &str) -> Callee {
    let n = name.trim();
    let (q, proc) = match n.split_once('(') {
        Some((q, p)) => (q.trim(), Some(p.trim_end_matches(')').trim().to_string())),
        None => (n, None),
    };
    let (lib, obj) = match q.split_once('/') {
        Some((l, o)) => (l.trim().to_ascii_uppercase(), o.trim().to_ascii_uppercase()),
        None => ("*LIBL".to_string(), q.to_ascii_uppercase()),
    };
    match proc {
        Some(p) => Callee::Proc(lib, obj, p),
        None => Callee::Pgm(lib, obj),
    }
}

/// Call a parser or generator with its one parameter: false when it
/// cannot be called.
pub(crate) fn call_callee(j: &mut mi::Job, c: &Callee, parm: u64) -> bool {
    match c {
        Callee::Pgm(lib, name) => j.call_program(name, lib, &[parm]).is_ok(),
        Callee::Proc(lib, srv, proc) => {
            let table = j.table.clone();
            let Ok(p) = j.act.load_program(srv, lib, ile::PgmKind::SrvPgm, &table) else { return false };
            let Some(addr) = p.export_addr(proc).or_else(|| p.export_addr(&proc.to_ascii_uppercase())) else { return false };
            mi::act::call_by_ref(j, addr, &[parm]);
            true
        }
        Callee::Ptr(addr) => {
            mi::act::call_by_ref(j, *addr, &[parm]);
            true
        }
    }
}

/// A buffer the called program reaches through a pointer: its space
/// pointer, and the buffer kept alive until the call is done.
pub(crate) struct Region {
    /// The storage.
    pub buf: Vec<u8>,
    base: u64,
}

impl Region {
    pub(crate) fn new(j: &mut mi::Job, buf: Vec<u8>) -> (Self, mi::FatPtr) {
        let mut buf = buf;
        if buf.is_empty() {
            buf.push(0);
        }
        let base = buf.as_mut_ptr() as u64;
        let p = j.spaces.add_region(base, buf.len() as u64);
        (Self { buf, base }, p)
    }

    pub(crate) fn drop_in(self, j: &mut mi::Job) {
        j.spaces.kill_at(self.base);
    }
}

/// How the second operand of `%PARSER` or `%GEN` was given.
pub(crate) enum UserParm {
    /// Not given.
    None,
    /// A string the runtime ends with a null.
    Text(String),
    /// A variable: its address, size and type (`QrnUserParmType_*`).
    Var(u64, usize, u8),
}

/// The user parameter from the compiler's operands: kind 0 none, 1 a
/// string, 2 a variable, 3 a data structure.
pub(crate) fn user_parm(addr: i64, desc: i64, kind: i32) -> UserParm {
    let d = crate::value::Desc::of(desc);
    match kind {
        1 => UserParm::Text(crate::xml::text_of(addr, desc)),
        2 => {
            let ty = match d.kind {
                crate::value::Kind::Char => b'3',
                crate::value::Kind::VarChar if d.prefix() == 4 => b'5',
                crate::value::Kind::VarChar => b'4',
                crate::value::Kind::Ind => b'2',
                _ => b'd',
            };
            UserParm::Var(addr.cast_unsigned(), d.size(), ty)
        }
        3 => UserParm::Var(addr.cast_unsigned(), d.size(), b'c'),
        _ => UserParm::None,
    }
}

/// Send the runtime's escape for an RPG status.
pub(crate) fn escape(j: &mut mi::Job, status: u16, reason: u32) {
    let id = format!("RNX{status:04}");
    let _ = j.send_message(&id, "QRNXMSG", "*LIBL", reason.to_be_bytes().to_vec(), qir::MsgType::Escape, mi::QueueRef::Same);
}

/// Run a parser over a document: the tree, or the failure (already
/// sent when it came from a procedure the parser called).
fn parse(j: &mut mi::Job, c: &Callee, doc: &str, o: &Options, up: &UserParm) -> Result<Builder, Option<Fail>> {
    let (data, ccsid) = if o.job_ccsid {
        (crate::value::ebcdic(doc), 0u32)
    } else {
        (doc.encode_utf16().flat_map(u16::to_be_bytes).collect::<Vec<u8>>(), 13488)
    };
    let data_len = u32::try_from(data.len()).unwrap_or(0);
    // A parser bases a large field on the document: room for one.
    let mut data = data;
    data.resize(data.len().max(DOC_ROOM), 0);
    let (doc_r, doc_p) = Region::new(j, data);
    let mut envb = vec![0u8; 31 * 16];
    for (i, a) in env_procs().iter().enumerate() {
        envb[i * 16..i * 16 + 16].copy_from_slice(&mi::FatPtr::procedure(*a as u64).to_bytes());
    }
    let (env_r, env_p) = Region::new(j, envb);
    let (h_r, h_p) = Region::new(j, vec![0; 16]);
    let mut up_r = None;
    let (up_p, is_str) = match up {
        UserParm::None => (mi::FatPtr::null(), false),
        UserParm::Text(t) => {
            let mut b = crate::value::ebcdic(t);
            b.push(0);
            let (r, p) = Region::new(j, b);
            up_r = Some(r);
            (p, true)
        }
        UserParm::Var(a, n, _) => {
            let depth = j.depth;
            (j.spaces.addr(*a, *n as u64, depth, false), false)
        }
    };
    let mut parm = vec![0u8; 144];
    parm[0..16].copy_from_slice(&doc_p.to_bytes());
    parm[16..32].copy_from_slice(&env_p.to_bytes());
    parm[32..48].copy_from_slice(&h_p.to_bytes());
    parm[48..64].copy_from_slice(&up_p.to_bytes());
    parm[64..68].copy_from_slice(&data_len.to_be_bytes());
    parm[68..72].copy_from_slice(&ccsid.to_be_bytes());
    parm[72] = if is_str { 0xF1 } else { 0xF0 };
    let (parm_r, _) = Region::new(j, parm);
    RUNS.with(|r| {
        r.borrow_mut().push(Run {
            b: Builder::default(),
            frames: j.act.frames.len(),
            data_ccsid: ccsid,
        });
    });
    let called = call_callee(j, c, parm_r.base);
    let run = RUNS.with(|r| r.borrow_mut().pop());
    for r in [Some(doc_r), Some(env_r), Some(h_r), Some(parm_r), up_r].into_iter().flatten() {
        r.drop_in(j);
    }
    let run = run.ok_or(None)?;
    if !called {
        j.clear();
        return Err(Some(fail(355, 1)));
    }
    if let Some(e) = run.b.error {
        // Sent already, to the caller.
        let _ = e;
        return Err(None);
    }
    if j.pending() {
        // The parser ended in an error of its own.
        return Err(None);
    }
    Ok(run.b)
}

/// `_QRNX_DATAINTO(target, shape, doc, dd, opts, od, name, nd, ptr, up,
/// ud, uk, handler, comm)`: parse and fill the target, or call the
/// handler; the number of elements.
pub extern "sysv64" fn data_into(
    env: *mut Env,
    _argc: i64,
    _od: i64,
    target: i64,
    shp: i64,
    doc: i64,
    dd: i64,
    opts: i64,
    od: i64,
    name: i64,
    nd: i64,
    ptr: i64,
    up: i64,
    ud: i64,
    uk: i32,
    h: i64,
    comm: i64,
) -> i64 {
    let j = mi::storage::job_of(env);
    let Some(s) = crate::xml::shape(&crate::xml::varchar_at(shp)) else { return 0 };
    let o = match options(&crate::xml::text_of(opts, od)) {
        Ok(o) => o,
        Err(e) => {
            escape(j, e.status, e.reason);
            return 0;
        }
    };
    let c = match crate::xml::proc_at(ptr) {
        Some(a) => Callee::Ptr(a),
        None => callee(&crate::xml::text_of(name, nd)),
    };
    let text = crate::xml::text_of(doc, dd);
    let user = user_parm(up, ud, uk);
    let b = match parse(j, &c, &text, &o, &user) {
        Ok(b) => b,
        Err(Some(e)) => {
            escape(j, e.status, e.reason);
            return 0;
        }
        Err(None) => return 0,
    };
    let Some(root) = b.root() else {
        escape(j, 356, 1);
        return 0;
    };
    let root = &root;
    let r = match crate::xml::proc_at(h) {
        Some(addr) => to_handler(j, addr, comm, &s, root, &o),
        None => into(target.cast_unsigned(), &s, root, &o),
    };
    r.unwrap_or_else(|e| {
        if e.status != 0 {
            escape(j, e.status, e.reason);
        }
        0
    })
}

/// `DATA-INTO %HANDLER`: the items in groups of the handler's array.
fn to_handler(j: &mut mi::Job, addr: usize, comm: i64, s: &Shape, root: &(String, Node), o: &Options) -> Result<i64, Fail> {
    let items = handler_items(root, s, o)?;
    let one = Shape { dim: 0, ..s.clone() };
    let group = usize::try_from(s.dim.max(1)).unwrap_or(1);
    let mut buf = vec![0u8; usize::try_from(s.stride).unwrap_or(0) * group];
    let mut total = 0i64;
    for chunk in items.chunks(group) {
        buf.fill(0);
        for (k, e) in chunk.iter().enumerate() {
            fill(buf.as_mut_ptr() as u64 + s.stride * k as u64, &one, e, o)?;
        }
        let n = i64::try_from(chunk.len()).unwrap_or(0);
        total += n;
        let mark = j.raised.len();
        let rc = mi::act::call_into_handler(j, addr, comm.cast_unsigned(), buf.as_mut_ptr() as u64, n);
        if j.pending() {
            crate::xml::handler_failed(j, mark);
            return Err(fail(0, 0));
        }
        if rc != 0 {
            break;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_tree() {
        let mut b = Builder::default();
        b.start().unwrap();
        b.start_struct().unwrap();
        b.name("id").unwrap();
        b.value("1").unwrap();
        b.name("tags").unwrap();
        b.start_array().unwrap();
        b.value("x").unwrap();
        b.value("y").unwrap();
        b.end_array().unwrap();
        b.end_struct().unwrap();
        b.finish().unwrap();
        let (n, node) = b.root().unwrap();
        assert!(n.is_empty());
        assert_eq!(
            node,
            Node::Struct(vec![
                ("id".into(), Node::Value("1".into())),
                ("tags".into(), Node::Array(vec![Node::Value("x".into()), Node::Value("y".into())])),
            ])
        );
    }

    #[test]
    fn misuse_has_its_reason() {
        let mut b = Builder::default();
        assert_eq!(b.name("x"), Err(fail(358, 8)));
        b.start().unwrap();
        assert_eq!(b.start(), Err(fail(358, 7)));
        b.start_struct().unwrap();
        assert_eq!(b.value("v"), Err(fail(358, 6)));
        assert_eq!(b.end_array(), Err(fail(358, 5)));
    }

    #[test]
    fn options_are_checked() {
        assert_eq!(options("bogus=yes"), Err(fail(352, 2)));
        assert_eq!(options("case=sideways"), Err(fail(352, 3)));
        assert_eq!(options("ccsid=any"), Err(fail(352, 3)));
        assert!(options("case=any countprefix=num_ allowmissing=yes").is_ok());
        assert_eq!(callee("MYLIB/SRV(proc)"), Callee::Proc("MYLIB".into(), "SRV".into(), "proc".into()));
        assert_eq!(callee("pgm"), Callee::Pgm("*LIBL".into(), "PGM".into()));
    }
}
