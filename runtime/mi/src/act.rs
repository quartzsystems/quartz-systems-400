// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Activation: programs loaded into memory, activation groups and the
//! activations inside them, and the call of a program entry procedure.
//! One environment serves the whole job; `enter` points its static
//! area at the running program's activation and `leave` restores it,
//! so a service program call needs no stub (`ABI.md` §2).

use std::collections::HashMap;
use std::sync::Arc;

use codegen::abi::Env;
use codegen::{Cond, Placed, RelocTarget};
use ile::{ActGrpAttr, ExportKind, ImportTarget, PgmKind, ProgramObject};

use crate::rtlib;

/// An activation group number; 0 is the default activation group.
pub type AgId = u32;

/// What kind of activation group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgKind {
    /// The default activation group, created at job start.
    Default,
    /// `*NEW`: one call's worth.
    New,
    /// A named group, until `RCLACTGRP`.
    Named,
}

/// A program's activation in a group: its static area.
pub struct Activation {
    /// The program.
    pub program: Arc<LoadedProgram>,
    /// The program's static area, every module's frame initialized.
    pub statics: Vec<u8>,
}

/// An activation group.
pub struct ActivationGroup {
    /// Number.
    pub id: AgId,
    /// Name: `*DFTACTGRP`, `*NEW` or the given name.
    pub name: String,
    /// Kind.
    pub kind: AgKind,
    /// Programs activated in it.
    pub activations: Vec<Activation>,
    /// Its heaps: the default heap first, then user-created ones.
    pub heaps: Vec<crate::heap::Heap>,
    /// Exit procedures (`CEE4RAGE`), called when the group is destroyed.
    pub exits: Vec<u64>,
    /// Alive; a reclaimed group's number is not reused.
    pub live: bool,
}

/// A program's code in memory, with its service programs.
pub struct LoadedProgram {
    /// `LIB/NAME`.
    pub key: String,
    /// The object.
    pub object: Arc<ProgramObject>,
    /// Each bound module's code.
    pub modules: Vec<codegen::Loaded>,
    /// Native address of each procedure, by module and procedure index.
    pub proc_addrs: Vec<Vec<usize>>,
    /// The service programs bound to, in the object's order.
    pub srvpgms: Vec<Arc<LoadedProgram>>,
    /// The debug variant is loaded.
    pub debug: bool,
}

impl LoadedProgram {
    /// The module and procedure a code address lies in.
    #[must_use]
    pub fn proc_of(&self, code: u64) -> Option<(u32, u32)> {
        for (mi, m) in self.modules.iter().enumerate() {
            let base = m.base() as u64;
            if code >= base && code < base + m.len() as u64 {
                // The map of the variant loaded: the debug one is larger.
                let pc = variant(&self.object.modules[mi], self.debug);
                let off = u32::try_from(code - base).ok()?;
                let p = pc.proc_at(off)?;
                return Some((u32::try_from(mi).ok()?, p.index));
            }
        }
        None
    }

    /// Whether a code address lies in this program.
    #[must_use]
    pub fn contains(&self, code: u64) -> bool {
        self.proc_of(code).is_some()
    }

    /// Native address of an exported procedure.
    #[must_use]
    pub fn export_addr(&self, name: &str) -> Option<usize> {
        let e = self.object.export_named(name)?;
        if e.kind != ExportKind::Proc {
            return None;
        }
        self.proc_addrs
            .get(e.module as usize)?
            .get(e.index as usize)
            .copied()
    }

    /// Native address of the program entry procedure.
    #[must_use]
    pub fn entry_addr(&self) -> Option<usize> {
        let (mi, pi) = self.object.entry?;
        self.proc_addrs.get(mi as usize)?.get(pi as usize).copied()
    }

    /// The procedure's name at a code address, for displays.
    #[must_use]
    pub fn proc_name(&self, code: u64) -> Option<String> {
        let (mi, pi) = self.proc_of(code)?;
        Some(
            self.object.modules[mi as usize].module.ir.procs[pi as usize]
                .name
                .clone(),
        )
    }
}

/// Where programs come from: the store, or a test's table.
pub trait ObjectSource {
    /// The program or service program `lib/name`; `lib` may be `*LIBL`.
    fn program(
        &mut self,
        name: &str,
        lib: &str,
        kind: PgmKind,
    ) -> Option<(Arc<ProgramObject>, String)>;
    /// The identifier a system pointer to `lib/name` of `obj_type`
    /// carries (`rslvsp`), or none when the object does not exist.
    fn resolve(&mut self, _name: &str, _lib: &str, _obj_type: &str) -> Option<u64> {
        None
    }
    /// The program a system pointer's identifier names, as `(name,
    /// library)`.
    fn by_id(&mut self, _id: u64) -> Option<(String, String)> {
        None
    }
    /// The programs (`LIB/NAME`) whose owners' authority the call stack
    /// now adopts, nearest first; the storage service checks them.
    fn adopt(&mut self, _programs: &[String]) {}
}

/// Programs held in memory, for tests and the IR driver.
#[derive(Default)]
pub struct MemSource {
    /// `(library, program)`.
    pub programs: Vec<(String, Arc<ProgramObject>)>,
}

impl MemSource {
    /// Add a program under a library.
    pub fn add(&mut self, lib: &str, p: ProgramObject) {
        self.programs.push((lib.into(), Arc::new(p)));
    }
}

impl ObjectSource for MemSource {
    fn program(
        &mut self,
        name: &str,
        lib: &str,
        kind: PgmKind,
    ) -> Option<(Arc<ProgramObject>, String)> {
        self.programs
            .iter()
            .find(|(l, p)| p.name == name && p.attr.kind == kind && (lib == "*LIBL" || l == lib))
            .map(|(l, p)| (p.clone(), l.clone()))
    }

    fn resolve(&mut self, name: &str, lib: &str, obj_type: &str) -> Option<u64> {
        let kind = match obj_type {
            "*PGM" => PgmKind::Pgm,
            "*SRVPGM" => PgmKind::SrvPgm,
            _ => return None,
        };
        self.programs
            .iter()
            .position(|(l, p)| {
                p.name == name && p.attr.kind == kind && (lib == "*LIBL" || l == lib)
            })
            .map(|i| u64::try_from(i + 1).expect("few programs"))
    }

    fn by_id(&mut self, id: u64) -> Option<(String, String)> {
        let i = usize::try_from(id.checked_sub(1)?).ok()?;
        self.programs
            .get(i)
            .map(|(l, p)| (p.name.clone(), l.clone()))
    }
}

/// The entry signature every program is called with.
type Entry = extern "sysv64" fn(*mut Env, i64, i64, *const u64);

/// A bound procedure with up to four by-reference parameters, as a
/// condition or cancel handler is called.
type Handler = extern "sysv64" fn(*mut Env, i64, i64, u64, u64, u64, u64);

/// Call a handler procedure with by-reference parameters.
/// Let the job escape before a call into generated code. The callee
/// reaches the job through the environment record, a pointer the
/// optimizer cannot see is derived from the caller's `&mut Job`; made
/// visible this way, stores before the call stay before it and loads
/// after it are not reused from before.
pub(crate) fn escape(j: &mut crate::job::Job) {
    std::hint::black_box(std::ptr::from_mut(j));
}

pub(crate) fn call_handler(addr: usize, env: *mut Env, params: [u64; 4], count: i64) {
    // SAFETY: `addr` is a procedure address the program handed the
    // runtime through a procedure pointer; the code generator gives
    // such procedures the bound signature `(env, argc, opdesc,
    // params...)` with by-reference parameters as addresses, and the
    // unused trailing registers are ignored by a callee with fewer.
    let f: Handler = unsafe { std::mem::transmute::<usize, Handler>(addr) };
    f(env, count, 0, params[0], params[1], params[2], params[3]);
}

/// An `XML-SAX` handler: the communication area, the event, the
/// string (a pointer by value), its length and the exception code.
/// Cranelift passes the pointer's halves on the stack when only one
/// register is left and leaves that register unused (`r9` here), so
/// the halves are spelled out after a filler.
type SaxHandler = extern "sysv64" fn(*mut Env, i64, i64, u64, i32, u64, u64, u64, i64, i32) -> i32;

/// An `XML-INTO` or `DATA-INTO` handler: the communication area, the
/// elements and their count.
type IntoHandler = extern "sysv64" fn(*mut Env, i64, i64, u64, u64, i64) -> i32;

/// Call an `XML-SAX` handler procedure; its return code.
pub fn call_sax_handler(j: &mut crate::job::Job, addr: usize, comm: u64, event: i32, string: u128, len: i64, exc: i32) -> i32 {
    let env = j.env();
    escape(j);
    // SAFETY: `addr` is the code address of a procedure the program
    // named with `%HANDLER`; the compiler checked its interface is the
    // handler's (`comm` by reference, then the event, the pointer, the
    // length and the exception by value), which the code generator
    // lowers to this signature.
    let f: SaxHandler = unsafe { std::mem::transmute::<usize, SaxHandler>(addr) };
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (string as u64, (string >> 64) as u64);
    let rc = f(env, 5, 0, comm, event, 0, lo, hi, len, exc);
    escape(j);
    rc
}

/// Call an `XML-INTO` or `DATA-INTO` handler procedure; its return code.
pub fn call_into_handler(j: &mut crate::job::Job, addr: usize, comm: u64, items: u64, n: i64) -> i32 {
    let env = j.env();
    escape(j);
    // SAFETY: as for `call_sax_handler`: the handler takes the
    // communication area and the elements by reference and the count
    // by value.
    let f: IntoHandler = unsafe { std::mem::transmute::<usize, IntoHandler>(addr) };
    let rc = f(env, 3, 0, comm, items, n);
    escape(j);
    rc
}

/// A bound procedure with up to six by-reference parameters.
type ByRef6 = extern "sysv64" fn(*mut Env, i64, i64, u64, u64, u64, u64, u64, u64);

/// Call a procedure a program named by a procedure pointer, with up to
/// six by-reference parameters (native addresses), as an API calls a
/// user exit procedure (`Qp0lProcessSubtree`).
///
/// # Panics
///
/// More than six parameters.
pub fn call_by_ref(j: &mut crate::job::Job, addr: usize, params: &[u64]) {
    assert!(params.len() <= 6, "at most six parameters");
    let mut p = [0u64; 6];
    p[..params.len()].copy_from_slice(params);
    let env = j.env();
    escape(j);
    // SAFETY: `addr` is the code address of a procedure pointer the
    // program passed; the code generator gives every bound procedure the
    // signature `(env, argc, opdesc, params...)` with by-reference
    // parameters as native addresses, and a callee with fewer
    // parameters ignores the trailing registers and stack slots.
    let f: ByRef6 = unsafe { std::mem::transmute::<usize, ByRef6>(addr) };
    f(env, i64::try_from(params.len()).unwrap_or(0), 0, p[0], p[1], p[2], p[3], p[4], p[5]);
    escape(j);
}

/// Call a program entry procedure with a parameter list.
pub(crate) fn call_entry(addr: usize, env: *mut Env, args: &[u64]) {
    // The stack check in `enter` measures from here.
    let marker = 0u8;
    let base = std::ptr::from_ref(&marker) as usize;
    if let Some(j) = crate::rtlib::job_opt(env)
        && j.stack_base == 0
    {
        j.stack_base = base;
    }
    // SAFETY: `addr` is the entry of a procedure the loader placed in
    // executable memory the job still holds; the code generator gave
    // every entry procedure the signature `(env, argc, opdesc, argv)`,
    // and `env` points to the job's live environment.
    let f: Entry = unsafe { std::mem::transmute::<usize, Entry>(addr) };
    f(
        env,
        i64::try_from(args.len()).expect("few"),
        0,
        args.as_ptr(),
    );
}

/// Load a program: place every module, resolve the service programs it
/// is bound to (recursively, checking signatures) and relocate.
pub(crate) fn load(
    object: Arc<ProgramObject>,
    key: String,
    table: &[usize],
    deps: Vec<Arc<LoadedProgram>>,
    system: &HashMap<String, usize>,
    debug: bool,
) -> Result<(Arc<LoadedProgram>, u32), Cond> {
    // Code made by an older code generator is translated again from
    // the IR the object keeps.
    let mut again = 0;
    let codes: Vec<std::borrow::Cow<'_, codegen::ModuleCode>> = object
        .modules
        .iter()
        .map(|bm| {
            let c = variant(bm, debug);
            if c.version == codegen::CODEGEN_VERSION {
                return Ok(std::borrow::Cow::Borrowed(c));
            }
            again += 1;
            let fresh = if debug && bm.module.debug_code.is_some() {
                codegen::compile_debug(&bm.module.ir)
            } else {
                codegen::compile(&bm.module.ir)
            }
            .map_err(|_| Cond::Mch3401)?;
            Ok(std::borrow::Cow::Owned(fresh))
        })
        .collect::<Result<_, Cond>>()?;
    let mut placed = Vec::with_capacity(object.modules.len());
    for c in &codes {
        placed.push(Placed::new(c).map_err(|_| Cond::Mch3401)?);
    }
    let proc_addrs: Vec<Vec<usize>> = placed
        .iter()
        .zip(&codes)
        .map(|(p, c)| {
            (0..c.procs.len())
                .map(|i| p.proc_addr_at(i).expect("procedure"))
                .collect()
        })
        .collect();
    let mut modules = Vec::with_capacity(placed.len());
    for (mi, (p, bm)) in placed.into_iter().zip(&object.modules).enumerate() {
        let mi32 = u32::try_from(mi).expect("few modules");
        let resolver = |t: &RelocTarget| -> codegen::Result<u64> {
            match t {
                RelocTarget::Runtime(f) => Ok(table[f.index() as usize] as u64),
                RelocTarget::AttnFlag => Ok(std::ptr::addr_of!(rtlib::ATTN) as u64),
                RelocTarget::StaticFrame => Ok(bm.frame_offset),
                RelocTarget::ImportProc(name) => {
                    let imp = object.imports.iter().find(|i| {
                        i.module == mi32 && i.name == *name && i.kind == ExportKind::Proc
                    });
                    let addr = match imp.map(|i| &i.target) {
                        Some(ImportTarget::Proc { module, proc }) => {
                            proc_addrs[*module as usize][*proc as usize]
                        }
                        Some(ImportTarget::SrvPgm { srvpgm, export }) => deps
                            .get(*srvpgm as usize)
                            .and_then(|d| d.export_addr(export))
                            .unwrap_or(rtlib::unresolved as *const () as usize),
                        Some(ImportTarget::System) => system
                            .get(name)
                            .copied()
                            .unwrap_or(rtlib::unresolved as *const () as usize),
                        _ => rtlib::unresolved as *const () as usize,
                    };
                    Ok(addr as u64)
                }
                RelocTarget::ImportData(name) => {
                    let imp = object.imports.iter().find(|i| {
                        i.module == mi32 && i.name == *name && i.kind == ExportKind::Data
                    });
                    match imp.map(|i| &i.target) {
                        Some(ImportTarget::Data { offset }) => Ok(*offset),
                        _ => Err(codegen::CodegenError::Load(format!("data import {name}"))),
                    }
                }
            }
        };
        modules.push(
            p.resolve(&codes[mi], &resolver)
                .map_err(|_| Cond::Mch3401)?,
        );
    }
    Ok((
        Arc::new(LoadedProgram {
            key,
            object,
            modules,
            proc_addrs,
            srvpgms: deps,
            debug,
        }),
        again,
    ))
}

/// Whether a program imports a procedure only system state may use.
#[must_use]
pub fn uses_privileged(object: &ProgramObject) -> bool {
    object.imports.iter().any(|i| {
        i.kind == ExportKind::Proc
            && i.target == ImportTarget::System
            && ile::binder::PRIVILEGED_PROCS.contains(&i.name.as_str())
    })
}

/// A bound module's code: the debug variant when asked for and present.
fn variant(bm: &ile::BoundModule, debug: bool) -> &codegen::ModuleCode {
    match (&bm.module.debug_code, debug) {
        (Some(dc), true) => dc,
        _ => &bm.module.code,
    }
}

/// A frame of the call stack as the runtime keeps it.
pub struct Frame {
    /// Code address of the procedure entered.
    pub code: u64,
    /// The program it belongs to, when known.
    pub program: Option<Arc<LoadedProgram>>,
    /// The activation group it runs in.
    pub ag: AgId,
    /// The caller's static area, restored on leave.
    pub saved_statics: *mut u8,
    /// The caller's activation group, restored on leave.
    pub saved_ag: AgId,
    /// The entry's call message queue.
    pub msgq: crate::msg::MsgQueue,
    /// A control boundary: a program entry procedure.
    pub control_boundary: bool,
    /// Arguments the entry was called with, `*OMIT` ones included.
    pub argc: u32,
    /// The operational descriptor list the caller passed, 0 for none.
    pub opdesc: u64,
    /// The `monitor` region the entry is suspended in, from the
    /// module's region table; 0 outside every region.
    pub region: u32,
    /// The statement the entry is suspended at.
    pub stmt: u32,
    /// ILE condition handlers (`CEEHDLR`), oldest first: procedure
    /// code address and token address.
    pub handlers: Vec<(u64, u64)>,
    /// Cancel handlers (`CEERTX`), oldest first.
    pub cancel_handlers: Vec<(u64, u64)>,
    /// Variables registered by `debug.var`, for `EVAL`.
    pub debug_vars: Vec<crate::debug::DbgVar>,
}

/// The activation state of a job.
pub struct Activator {
    /// Loaded programs by `LIB/NAME`.
    pub loaded: HashMap<String, Arc<LoadedProgram>>,
    /// Activation groups by number; the default is first.
    pub groups: Vec<ActivationGroup>,
    /// The group the running code is in.
    pub current: AgId,
    /// The call stack.
    pub frames: Vec<Frame>,
    /// Where programs come from.
    pub source: Box<dyn ObjectSource>,
    /// Bound procedures the runtime provides (`*SYSTEM` imports), by
    /// export name: the condition APIs, and what `sysapi` installs.
    pub system: HashMap<String, usize>,
    /// Programs (`LIB/NAME`) that load their debug variant.
    pub debug_programs: Vec<String>,
    /// Modules translated again at load because their code was made
    /// by an older code generator.
    pub retranslated: u32,
    /// Frames whose program adopts (`USRPRF(*OWNER)`).
    pub adopting_frames: usize,
    /// The adopting programs last told to the source.
    pub adopted: Vec<String>,
    /// `QSECURITY`; 40 and up enforce domain and state.
    pub qsecurity: u8,
}

impl Activator {
    /// A job's activation state with its default activation group.
    pub fn new(source: Box<dyn ObjectSource>) -> Self {
        Self {
            loaded: HashMap::new(),
            groups: vec![ActivationGroup {
                id: 0,
                name: "*DFTACTGRP".into(),
                kind: AgKind::Default,
                activations: Vec::new(),
                heaps: vec![crate::heap::Heap::new(0)],
                exits: Vec::new(),
                live: true,
            }],
            current: 0,
            frames: Vec::new(),
            source,
            system: crate::cee::NAMES
                .iter()
                .filter_map(|n| crate::cee::bound_api(n).map(|a| ((*n).to_string(), a)))
                .collect(),
            debug_programs: Vec::new(),
            retranslated: 0,
            adopting_frames: 0,
            adopted: Vec::new(),
            qsecurity: 0,
        }
    }

    /// Whether the running entry is in user state: its program's state;
    /// command level (no entry) is the system's `QCMD`.
    #[must_use]
    pub fn user_state(&self) -> bool {
        self.frames
            .last()
            .and_then(|f| f.program.as_ref())
            .is_some_and(|p| p.object.attr.user_state)
    }

    /// Whether domain and state are enforced.
    #[must_use]
    pub fn enforcing(&self) -> bool {
        self.qsecurity >= 40
    }

    /// The programs whose owners' authority applies now, nearest first:
    /// each `USRPRF(*OWNER)` program on the stack, until a program with
    /// `USEADPAUT(*NO)` stops what is below it from reaching above.
    #[must_use]
    pub fn adopting(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut usable = true;
        let mut last: Option<&str> = None;
        for f in self.frames.iter().rev() {
            let Some(p) = &f.program else {
                continue;
            };
            if last == Some(p.key.as_str()) {
                continue;
            }
            last = Some(p.key.as_str());
            if usable && p.object.attr.usrprf == ile::UserProfile::Owner && !out.contains(&p.key) {
                out.push(p.key.clone());
            }
            if !p.object.attr.useadpaut {
                usable = false;
            }
        }
        out
    }

    /// A frame came or went for `program`: tell the source when what is
    /// adopted changed. Costs nothing while no program adopts.
    pub fn frame_changed(&mut self, program: Option<&Arc<LoadedProgram>>, pushed: bool) {
        let owner = program.is_some_and(|p| p.object.attr.usrprf == ile::UserProfile::Owner);
        if owner {
            if pushed {
                self.adopting_frames += 1;
            } else {
                self.adopting_frames = self.adopting_frames.saturating_sub(1);
            }
        }
        if !owner && self.adopting_frames == 0 && self.adopted.is_empty() {
            return;
        }
        let now = self.adopting();
        if now != self.adopted {
            self.source.adopt(&now);
            self.adopted = now;
        }
    }

    /// The loaded program a code address belongs to.
    #[must_use]
    pub fn program_of(&self, code: u64) -> Option<Arc<LoadedProgram>> {
        self.loaded.values().find(|p| p.contains(code)).cloned()
    }

    /// Create a group.
    pub fn create_group(&mut self, name: &str, kind: AgKind) -> AgId {
        let id = u32::try_from(self.groups.len()).expect("few groups");
        self.groups.push(ActivationGroup {
            id,
            name: name.into(),
            kind,
            activations: Vec::new(),
            heaps: vec![crate::heap::Heap::new(0)],
            exits: Vec::new(),
            live: true,
        });
        id
    }

    /// The named group, created when absent.
    pub fn named_group(&mut self, name: &str) -> AgId {
        if let Some(g) = self
            .groups
            .iter()
            .find(|g| g.live && g.kind == AgKind::Named && g.name == name)
        {
            return g.id;
        }
        self.create_group(name, AgKind::Named)
    }

    /// The group a program runs in when called from `caller`.
    pub fn group_for(&mut self, p: &ProgramObject, caller: AgId) -> AgId {
        match &p.attr.actgrp {
            ActGrpAttr::Caller | ActGrpAttr::New => caller,
            ActGrpAttr::Default => 0,
            ActGrpAttr::Named(n) => self.named_group(n),
        }
    }

    /// The activation of a program in a group, made on first use; the
    /// static area's address.
    pub fn activate(&mut self, p: &Arc<LoadedProgram>, ag: AgId) -> (*mut u8, usize) {
        let g = &mut self.groups[ag as usize];
        if let Some(a) = g
            .activations
            .iter_mut()
            .find(|a| Arc::ptr_eq(&a.program, p))
        {
            return (a.statics.as_mut_ptr(), a.statics.len());
        }
        let mut statics = vec![0u8; usize::try_from(p.object.static_size).expect("fits").max(16)];
        for bm in &p.object.modules {
            let at = usize::try_from(bm.frame_offset).expect("fits");
            let init = &bm.module.code.static_init;
            statics[at..at + init.len()].copy_from_slice(init);
        }
        g.activations.push(Activation {
            program: p.clone(),
            statics,
        });
        let a = g.activations.last_mut().expect("pushed");
        (a.statics.as_mut_ptr(), a.statics.len())
    }

    /// Load a program and the service programs it is bound to; the
    /// signature each was bound with must be one the service program
    /// accepts (`MCH4431`).
    pub fn load_program(
        &mut self,
        name: &str,
        lib: &str,
        kind: PgmKind,
        table: &[usize],
    ) -> Result<Arc<LoadedProgram>, Cond> {
        let (object, found_lib) = self.source.program(name, lib, kind).ok_or(Cond::Mch3401)?;
        let key = format!("{found_lib}/{name}");
        let debug = self.debug_programs.contains(&key);
        if let Some(p) = self.loaded.get(&key) {
            if p.debug == debug {
                return Ok(p.clone());
            }
            self.loaded.remove(&key);
        }
        if self.enforcing() && object.attr.user_state && uses_privileged(&object) {
            return Err(Cond::Mch6801);
        }
        let mut deps = Vec::with_capacity(object.srvpgms.len());
        for r in &object.srvpgms {
            let dep = self.load_program(&r.name, &r.lib, PgmKind::SrvPgm, table)?;
            if !dep.object.accepts(r.signature) {
                return Err(Cond::Mch4431);
            }
            deps.push(dep);
        }
        let (p, again) = load(object, key.clone(), table, deps, &self.system, debug)?;
        self.retranslated += again;
        self.loaded.insert(key, p.clone());
        Ok(p)
    }

    /// `RCLACTGRP`: destroy a named group; the static areas of its
    /// activations go away. A group with a frame on the call stack
    /// cannot be reclaimed.
    pub fn reclaim(&mut self, name: &str) -> Result<Vec<u64>, &'static str> {
        let Some(id) = self
            .groups
            .iter()
            .find(|g| g.live && g.kind != AgKind::Default && g.name == name)
            .map(|g| g.id)
        else {
            return Err("no such activation group");
        };
        self.reclaim_id(id)
    }

    /// Destroy a group by number; returns the static areas it freed.
    pub fn reclaim_id(&mut self, id: AgId) -> Result<Vec<u64>, &'static str> {
        if self.frames.iter().any(|f| f.ag == id) {
            return Err("the activation group is in use");
        }
        let g = &mut self.groups[id as usize];
        g.live = false;
        let mut bases: Vec<u64> = g
            .activations
            .iter()
            .map(|a| a.statics.as_ptr() as u64)
            .collect();
        g.activations.clear();
        for h in g.heaps.drain(..) {
            bases.extend(h.blocks.iter().map(|b| b.data.as_ptr() as u64));
        }
        Ok(bases)
    }

    /// `RCLACTGRP(*ELIGIBLE)`: every named group with no frame on the stack.
    pub fn reclaim_eligible(&mut self) -> Vec<u64> {
        let ids: Vec<AgId> = self
            .groups
            .iter()
            .filter(|g| g.live && g.kind == AgKind::Named)
            .map(|g| g.id)
            .collect();
        let mut out = Vec::new();
        for id in ids {
            if let Ok(b) = self.reclaim_id(id) {
                out.extend(b);
            }
        }
        out
    }

    /// Live groups, for `DSPACTGRP`.
    pub fn groups(&self) -> impl Iterator<Item = &ActivationGroup> {
        self.groups.iter().filter(|g| g.live)
    }
}
