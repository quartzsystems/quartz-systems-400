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
}

impl LoadedProgram {
    /// The module and procedure a code address lies in.
    #[must_use]
    pub fn proc_of(&self, code: u64) -> Option<(u32, u32)> {
        for (mi, m) in self.modules.iter().enumerate() {
            let base = m.base() as u64;
            if code >= base && code < base + m.len() as u64 {
                let pc = &self.object.modules[mi].module.code;
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
}

/// The entry signature every program is called with.
type Entry = extern "sysv64" fn(*mut Env, i64, i64, *const u64);

/// Call a program entry procedure with a parameter list.
pub(crate) fn call_entry(addr: usize, env: *mut Env, args: &[u64]) {
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
) -> Result<Arc<LoadedProgram>, Cond> {
    let mut placed = Vec::with_capacity(object.modules.len());
    for bm in &object.modules {
        placed.push(Placed::new(&bm.module.code).map_err(|_| Cond::Mch3401)?);
    }
    let proc_addrs: Vec<Vec<usize>> = placed
        .iter()
        .zip(&object.modules)
        .map(|(p, bm)| {
            (0..bm.module.code.procs.len())
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
            p.resolve(&bm.module.code, &resolver)
                .map_err(|_| Cond::Mch3401)?,
        );
    }
    Ok(Arc::new(LoadedProgram {
        key,
        object,
        modules,
        proc_addrs,
        srvpgms: deps,
    }))
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
                live: true,
            }],
            current: 0,
            frames: Vec::new(),
            source,
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
        if let Some(p) = self.loaded.get(&key) {
            return Ok(p.clone());
        }
        let mut deps = Vec::with_capacity(object.srvpgms.len());
        for r in &object.srvpgms {
            let dep = self.load_program(&r.name, &r.lib, PgmKind::SrvPgm, table)?;
            if !dep.object.accepts(r.signature) {
                return Err(Cond::Mch4431);
            }
            deps.push(dep);
        }
        let p = load(object, key.clone(), table, deps)?;
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
        let bases: Vec<u64> = g
            .activations
            .iter()
            .map(|a| a.statics.as_ptr() as u64)
            .collect();
        g.activations.clear();
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
