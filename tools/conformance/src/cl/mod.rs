// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CL conformance corpus: `conformance/cl/programs/*.clle`, each a
//! program compiled against the shipped command definitions and run
//! in the IR kit's job. Every processing program is a test double
//! that records the command it was called with, in keyword form, on
//! the job's console, and fills return values with marks (`RTNVAL`
//! parameters get their keyword as text, 7 as a number, `'1'` as a
//! logical). The console, the messages and the parameters render as
//! the IR kit renders them, and the listing goes beside the outcome.
//!
//! Header comments steer a case, as the IR kit's do:
//! `/*; parms: 'abc' 12 */`, `/*; replies: C G */`, `/*; dsply: yes */`
//! and `/*; fail: CRTLIB CPF2111 */` (the double of `CRTLIB` escapes
//! with `CPF2111`; several may be given).

use std::cell::RefCell;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cmd::analyze::{Bound, Env, MemSource, Setting};
use cmd::block;
use cmd::def::ParmType;
use cmd::{CmdDef, Value};
use mi::{Job, QueueRef};
use qir::MsgType;

use crate::qir as irkit;

/// Where the corpus lives.
pub struct Kit {
    /// `conformance/cl`.
    pub root: PathBuf,
}

impl Kit {
    /// The corpus checked in beside this crate.
    #[must_use]
    pub fn locate() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("cl"),
        }
    }

    /// Every case, in name order.
    pub fn cases(&self) -> std::io::Result<Vec<Case>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.root.join("programs"))? {
            let p = entry?.path();
            if p.extension().is_some_and(|e| e == "clle") {
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let text = fs::read_to_string(&p)?.replace("\r\n", "\n");
                out.push(Case::parse(&name, &text));
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// `expected/`.
    #[must_use]
    pub fn expected_dir(&self) -> PathBuf {
        self.root.join("expected")
    }

    /// Where a run writes its outputs.
    #[must_use]
    pub fn ours_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join("conformance-cl")
            .join("ours")
    }
}

/// One case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    /// File stem, upper case.
    pub name: String,
    /// The source.
    pub source: String,
    /// Parameters as the console's `call` types them.
    pub parms: String,
    /// Inquiry replies.
    pub replies: Vec<String>,
    /// `dsply` replies.
    pub dsply: Vec<String>,
    /// Commands whose doubles escape, with the message.
    pub fail: Vec<(String, String)>,
}

impl Case {
    /// Read the header comments.
    #[must_use]
    pub fn parse(name: &str, text: &str) -> Self {
        let mut c = Self {
            name: name.to_ascii_uppercase(),
            source: text.to_string(),
            ..Self::default()
        };
        for l in text.lines() {
            let Some(h) = l.trim().strip_prefix("/*;") else {
                continue;
            };
            let h = h.trim_end_matches("*/").trim();
            if let Some(r) = h.strip_prefix("parms:") {
                c.parms = r.trim().to_string();
            } else if let Some(r) = h.strip_prefix("replies:") {
                c.replies = r.split_whitespace().map(str::to_string).collect();
            } else if let Some(r) = h.strip_prefix("dsply:") {
                c.dsply = r.split_whitespace().map(str::to_string).collect();
            } else if let Some(r) = h.strip_prefix("fail:") {
                let mut w = r.split_whitespace();
                if let (Some(cmd), Some(id)) = (w.next(), w.next()) {
                    c.fail
                        .push((cmd.to_ascii_uppercase(), id.to_ascii_uppercase()));
                }
            }
        }
        c
    }
}

/// The shipped definitions.
#[must_use]
pub fn defs() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        if let Ok(o) = sys_cpps::install::object(name, irkit::CLOCK) {
            s.put(o.def);
        }
    }
    s
}

/// The kit's files: `CUST` as the IR kit creates it, and `KITDSP`, a
/// display file with one format.
pub struct KitFiles;

impl cl::Files for KitFiles {
    fn file(&mut self, name: &str, _lib: &str) -> Option<(bool, Vec<cl::Format>)> {
        let f = |n: &str, ty: char, pos: u32, len: u32, digits: u32, decimals: u32| cl::FieldDef {
            name: n.into(),
            ty,
            pos,
            len,
            digits,
            decimals,
        };
        match name {
            "CUST" => Some((
                false,
                vec![cl::Format {
                    name: "CUSREC".into(),
                    len: 39,
                    fields: vec![
                        f("CUSNO", 'P', 1, 4, 7, 0),
                        f("NAME", 'A', 5, 30, 0, 0),
                        f("BAL", 'P', 35, 5, 9, 2),
                    ],
                }],
            )),
            "KITDSP" => Some((
                true,
                vec![cl::Format {
                    name: "PROMPT".into(),
                    len: 20,
                    fields: vec![f("ANSWER", 'A', 1, 20, 0, 0)],
                }],
            )),
            _ => None,
        }
    }
}

/// Compile a case.
#[must_use]
pub fn compile(case: &Case) -> cl::Compiled {
    let mut src = defs();
    let mut env = Env::new(&mut src);
    env.setting = Setting {
        interactive: false,
        program: true,
        exec: false,
        module: false,
    };
    let opts = cl::Options {
        name: case.name.clone(),
        srcfile: "QCLSRC".into(),
        srclib: "QGPL".into(),
        srcmbr: case.name.clone(),
        text: format!("corpus {}", case.name),
        ..cl::Options::default()
    };
    cl::compile(&case.source, &opts, &mut env, &mut KitFiles)
}

/// What the doubles share.
struct Doubles {
    defs: RefCell<MemSource>,
    fail: Vec<(String, String)>,
}

impl Doubles {
    fn definition(&self, name: &str) -> Option<CmdDef> {
        let mut d = self.defs.borrow_mut();
        let (o, _) = cmd::analyze::CmdSource::lookup(&mut *d, name, Some("QSYS")).ok()??;
        Some(o.effective())
    }

    /// The defaults of a command, to tell given values apart.
    fn defaults(&self, def: &CmdDef) -> Vec<Bound> {
        let mut d = self.defs.borrow_mut();
        let mut env = Env::new(&mut *d);
        env.setting.program = true;
        let bounds = cmd::analyze::defaults(&mut env, def);
        // Through the block and back, as the call's values came.
        let Ok(built) = block::build(def, &bounds) else {
            return bounds;
        };
        let spec = vec![false; def.parms.len()];
        block::read(def, &built.segments, &spec).map_or(bounds, |p| p.values)
    }

    /// The keyword form of a call: the defaults and the return
    /// variables left out.
    fn line(&self, def: &CmdDef, parms: &[Bound]) -> String {
        let defaults = self.defaults(def);
        let mut s = def.name.clone();
        for (i, b) in parms.iter().enumerate() {
            let Some(v) = &b.value else {
                continue;
            };
            if def.parms.get(i).is_some_and(|p| p.rtnval) {
                continue;
            }
            let dft = defaults.get(i).and_then(|d| d.value.as_ref());
            let text = v.parm_source();
            if dft.is_some_and(|d| d.source() == v.source()) || text.is_empty() {
                continue;
            }
            let _ = write!(s, " {}({})", b.kwd, text);
        }
        s
    }

    fn escape_for(&self, name: &str) -> Option<&str> {
        self.fail
            .iter()
            .find(|(c, _)| c == name)
            .map(|(_, id)| id.as_str())
    }
}

fn escape(job: &mut Job, msgid: &str, name: &str) {
    let data = ccsid::to_ebcdic(name).unwrap_or_default();
    // A native has no frame of its own: the program's queue is the
    // one on top, and the escape lands as its processing program's.
    let _ = job.send_message(
        msgid,
        "QCPFMSG",
        "QSYS",
        data,
        MsgType::Escape,
        QueueRef::Same,
    );
}

/// A return value for a parameter: its keyword, 7 or `'1'`.
fn mark(p: &cmd::def::Parm) -> Option<Value> {
    Some(match p.ty {
        ParmType::Dec | ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => {
            Value::Num(qdecimal::Decimal::new(7, 0).ok()?)
        }
        ParmType::Lgl => Value::Char("1".into()),
        ParmType::Qual { .. } | ParmType::Elem { .. } => return None,
        _ => Value::Char(p.kwd.clone()),
    })
}

/// The double of one processing program.
fn cpp(job: &mut Job, d: &Doubles, name: &str, args: &[u64]) {
    let Some(def) = d.definition(name) else {
        job.console.push(format!("> {name}: no definition"));
        return;
    };
    let mut segments = Vec::with_capacity(def.parms.len());
    for (i, p) in def.parms.iter().enumerate() {
        let addr = args.get(i).copied().unwrap_or(0);
        if addr == 0 {
            segments.push(None);
        } else {
            let n = block::segment_len(p, &mut |off, k| {
                mi::storage::view(addr + off as u64, k).to_vec()
            });
            segments.push(Some(mi::storage::view(addr, n).to_vec()));
        }
    }
    let specified = vec![true; def.parms.len()];
    let parms = match block::read(&def, &segments, &specified) {
        Ok(p) => p,
        Err(e) => {
            job.console.push(format!("> {name}: {e}"));
            return;
        }
    };
    job.console.push(format!("> {}", d.line(&def, &parms.values)));
    if let Some(id) = d.escape_for(name) {
        escape(job, id, name);
        return;
    }
    // Return values.
    let mut back = parms.values.clone();
    let mut any = false;
    for (i, p) in def.parms.iter().enumerate() {
        if p.rtnval && let Some(v) = mark(p) {
            back[i].value = Some(v);
            any = true;
        }
    }
    if !any {
        return;
    }
    let Ok(built) = block::build(&def, &back) else {
        return;
    };
    for (i, p) in def.parms.iter().enumerate() {
        if !p.rtnval {
            continue;
        }
        let (Some(addr), Some(Some(seg))) = (args.get(i).copied(), built.segments.get(i)) else {
            continue;
        };
        let held = segments.get(i).and_then(|s| s.as_ref()).map_or(0, Vec::len);
        let n = seg.len().min(held);
        if addr != 0 && n > 0 {
            mi::storage::view(addr, n).copy_from_slice(&seg[..n]);
        }
    }
}

/// `QCLEXEC`: the template filled and analyzed.
fn clexec(job: &mut Job, d: &Doubles, args: &[u64]) {
    let read = |i: usize, n: usize| -> Vec<u8> {
        match args.get(i).copied() {
            Some(a) if a != 0 => mi::storage::view(a, n).to_vec(),
            _ => Vec::new(),
        }
    };
    let packed = |b: &[u8], digits: u8, scale: u8| -> Option<qdecimal::Decimal> {
        qdecimal::Packed::new(digits, scale).ok()?.decode(b).ok()
    };
    let number = |b: &[u8]| -> usize {
        packed(b, 10, 0)
            .and_then(|x| x.mantissa_i128())
            .and_then(|x| usize::try_from(x).ok())
            .unwrap_or(0)
    };
    let len = number(&read(0, 6));
    let mut text = ccsid::from_ebcdic(&read(1, len));
    let count = number(&read(2, 6));
    for k in 0..count {
        let desc = ccsid::from_ebcdic(&read(3 + 2 * k, 16));
        let code = desc.chars().next().unwrap_or('C');
        let width: usize = desc
            .get(1..6)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let scale: u8 = desc
            .get(6..8)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let value = match code {
            'D' => {
                let digits = u8::try_from(width).unwrap_or(15);
                let bytes = read(4 + 2 * k, usize::from(digits) / 2 + 1);
                packed(&bytes, digits, scale).map_or_else(|| "0".into(), |x| x.to_string())
            }
            'L' => {
                if read(4 + 2 * k, 1).first() == Some(&0xF1) {
                    "'1'".into()
                } else {
                    "'0'".into()
                }
            }
            _ => {
                let s = ccsid::from_ebcdic(&read(4 + 2 * k, width));
                let s = s.trim_end();
                if cmd::value::needs_quotes(s) {
                    format!("'{}'", s.replace('\'', "''"))
                } else {
                    s.to_string()
                }
            }
        };
        text = text.replace(&format!("&{}", k + 1), &value);
    }
    let r = {
        let mut defs = d.defs.borrow_mut();
        let mut env = Env::new(&mut *defs);
        env.setting.program = true;
        cmd::analyze(&mut env, &text)
    };
    match r {
        Ok(r) => {
            job.console.push(format!(">> {}", d.line(&r.def, &r.parms)));
            if let Some(id) = d.escape_for(&r.name) {
                escape(job, id, &r.name);
            }
        }
        Err(e) => {
            let id = e
                .diags
                .first()
                .map_or_else(|| e.msgid().to_string(), |x| x.msgid.clone());
            job.console.push(format!(">> {text}: {id}"));
            escape(job, &id, &text.chars().take(10).collect::<String>());
        }
    }
}

/// Install the doubles.
fn install(job: &mut Job, d: &Rc<Doubles>) {
    for e in sys_cpps::all() {
        let d = d.clone();
        let name = e.name.clone();
        job.natives.insert(
            e.cpp.clone(),
            Rc::new(move |job: &mut Job, args: &[u64]| cpp(job, &d, &name, args)),
        );
    }
    let dd = d.clone();
    job.natives.insert(
        "QCLEXEC".into(),
        Rc::new(move |job: &mut Job, args: &[u64]| clexec(job, &dd, args)),
    );
    job.natives.insert(
        "QCLSWS".into(),
        Rc::new(|_job: &mut Job, args: &[u64]| {
            let (Some(m), Some(r)) = (args.first().copied(), args.get(1).copied()) else {
                return;
            };
            if m == 0 || r == 0 {
                return;
            }
            let mask = ccsid::from_ebcdic(mi::storage::view(m, 8));
            let ok = mask.chars().all(|c| c == 'X' || c == '0');
            mi::storage::view(r, 1)[0] = if ok { 0xF1 } else { 0xF0 };
        }),
    );
    job.natives.insert(
        "QCLDMP".into(),
        Rc::new(|job: &mut Job, _args: &[u64]| {
            job.last_dump = Some(job.dump(true));
        }),
    );
}

/// Compile and run a case; the rendered outcome and the listing.
pub fn run(case: &Case) -> Result<(String, Vec<String>), String> {
    let c = compile(case);
    if !c.created {
        let mut out = String::from("not created:\n");
        for d in &c.diags {
            let _ = writeln!(out, "  {d}");
        }
        return Ok((out, c.listing));
    }
    let p = irkit::program(&c.qir).map_err(|e| format!("{}: {e}\n{}", case.name, c.qir))?;
    let mut src = mi::MemSource::default();
    let main = p.name.clone();
    src.add("QGPL", p);
    let ir_case = irkit::Case {
        name: case.name.clone(),
        modules: Vec::new(),
        replies: case.replies.clone(),
        dsply: case.dsply.clone(),
        parms: case.parms.clone(),
        qemu: false,
    };
    let mut job = irkit::job_for(&ir_case, src, std::slice::from_ref(&main))?;
    let d = Rc::new(Doubles {
        defs: RefCell::new(defs()),
        fail: case.fail.clone(),
    });
    install(&mut job, &d);
    let mut out = String::new();
    for d in &c.diags {
        let _ = writeln!(out, "diag: {d}");
    }
    if std::env::var_os("CL_DUMP").is_some() {
        eprintln!("{}", c.qir);
    }
    let (text, ext) = call(&ir_case, job, &main)?;
    Ok((out + &text + &ext, c.listing))
}

/// The IR kit's call, with the external message queue rendered after
/// it: what a user at Command Entry would have seen.
fn call(ir_case: &irkit::Case, mut job: Box<Job>, main: &str) -> Result<(String, String), String> {
    let ext = Rc::new(RefCell::new(Vec::new()));
    let seen = ext.clone();
    // The queue is read after the call through a native the program
    // never calls; the job is consumed by the kit's call.
    job.natives.insert(
        "QCLKITEXT".into(),
        Rc::new(move |job: &mut Job, _args: &[u64]| {
            let mut v = seen.borrow_mut();
            v.clear();
            for m in &job.ext_msgq.msgs {
                v.push(format!("  {} {}", m.msgid, m.text.trim_end()));
            }
        }),
    );
    let text = irkit::call_with(ir_case, job, main, &|job: &mut Job| {
        if let Some(f) = job.natives.get("QCLKITEXT").cloned() {
            f(job, &[]);
        }
    })?;
    let lines = ext.borrow();
    let mut out = String::from("ext:\n");
    for l in lines.iter() {
        out.push_str(l);
        out.push('\n');
    }
    Ok((text, out))
}
