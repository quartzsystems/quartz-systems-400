// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lowering: IR procedures to Cranelift functions to native code.
//! Registers become Cranelift variables, blocks become blocks, places
//! become addresses, and everything that can raise a machine condition
//! calls the runtime and then polls the job's pending flag.

mod call;
mod inst;
mod place;
mod storage;

use std::collections::HashMap;

use cranelift_codegen::FinalizedRelocTarget;
use cranelift_codegen::binemit::Reloc as ClReloc;
use cranelift_codegen::control::ControlPlane;
use cranelift_codegen::ir::Endianness;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::{
    Block, ExtFuncData, ExternalName, FuncRef, Function, GlobalValue, GlobalValueData,
    MemFlagsData, SourceLoc, StackSlot, StackSlotData, StackSlotKind, UserExternalName,
    UserFuncName, Value, types,
};
use cranelift_codegen::isa::OwnedTargetIsa;
use cranelift_codegen::settings::Configurable;
use cranelift_codegen::{Context, settings};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use qir::module::index32;
use qir::{Module, Proc, StorageClass, Type};

use crate::abi::{self, StaticLayout, off32, usz};
use crate::error::{CodegenError, Result};
use crate::output::{
    CODEGEN_VERSION, ModuleCode, ProcCode, Reloc, RelocKind, RelocTarget, StmtRange,
};
use crate::rt::{Cond, RtFn};

/// External name namespaces.
const NS_LOCAL: u32 = 0;
const NS_RT: u32 = 1;
const NS_IMPORT_PROC: u32 = 2;
const NS_STATIC_FRAME: u32 = 3;
const NS_IMPORT_DATA: u32 = 4;

/// The target: `x86_64`, System V, no position independence, no probes.
pub fn target() -> Result<OwnedTargetIsa> {
    let mut fb = settings::builder();
    let set = |fb: &mut settings::Builder, k: &str, v: &str| {
        fb.set(k, v)
            .map_err(|e| CodegenError::Target(format!("{k}: {e}")))
    };
    set(&mut fb, "opt_level", "speed")?;
    set(&mut fb, "is_pic", "false")?;
    set(&mut fb, "enable_probestack", "false")?;
    set(&mut fb, "use_colocated_libcalls", "false")?;
    // Fat pointers travel as i128 values; the System V extension passes
    // and returns them in register pairs, as Rust does for u128.
    set(&mut fb, "enable_llvm_abi_extensions", "true")?;
    set(
        &mut fb,
        "enable_verifier",
        if cfg!(debug_assertions) {
            "true"
        } else {
            "false"
        },
    )?;
    let flags = settings::Flags::new(fb);
    let triple: target_lexicon::Triple = "x86_64-unknown-linux-musl"
        .parse()
        .map_err(|e| CodegenError::Target(format!("{e}")))?;
    cranelift_codegen::isa::lookup(triple)
        .map_err(|e| CodegenError::Target(e.to_string()))?
        .finish(flags)
        .map_err(|e| CodegenError::Target(e.to_string()))
}

/// Compile a verified module.
#[allow(clippy::too_many_lines)]
pub fn compile(m: &Module) -> Result<ModuleCode> {
    qir::verify(m).map_err(|e| CodegenError::Verify(e.to_string()))?;
    let isa = target()?;
    let layout = StaticLayout::of(m);
    let mut names: Vec<String> = Vec::new();
    let mut code: Vec<u8> = Vec::new();
    let mut relocs: Vec<Reloc> = Vec::new();
    let mut procs: Vec<ProcCode> = Vec::new();
    let mut pending_local: Vec<(usize, ClReloc, u32, i64)> = Vec::new();

    for (pi, p) in m.procs.iter().enumerate() {
        let mut func = Function::with_name_signature(
            UserFuncName::user(NS_LOCAL, index32(pi)),
            abi::signature(&p.params, p.ret.as_ref()),
        );
        let mut fctx = FunctionBuilderContext::new();
        {
            let b = FunctionBuilder::new(&mut func, &mut fctx);
            let mut lw = Lowerer::new(m, p, index32(pi), b, &layout, &mut names);
            lw.lower()?;
        }
        let mut ctx = Context::for_function(func);
        let user_names = ctx.func.params.user_named_funcs().clone();
        let compiled = ctx
            .compile(&*isa, &mut ControlPlane::default())
            .map_err(|e| CodegenError::Backend {
                proc: p.name.clone(),
                msg: format!("{e:?}"),
            })?;
        // Align each procedure at 16.
        while !code.len().is_multiple_of(16) {
            code.push(0xCC);
        }
        let start = code.len();
        let bytes = compiled.code_buffer();
        code.extend_from_slice(bytes);
        for r in compiled.buffer.relocs() {
            let at = start + r.offset as usize;
            match &r.target {
                FinalizedRelocTarget::ExternalName(ExternalName::User(nr)) => {
                    let name = user_names[*nr].clone();
                    let target = match name.namespace {
                        NS_LOCAL => {
                            pending_local.push((at, r.kind, name.index, r.addend));
                            continue;
                        }
                        NS_RT => RelocTarget::Runtime(
                            RtFn::from_index(name.index).expect("known runtime function"),
                        ),
                        NS_IMPORT_PROC => {
                            RelocTarget::ImportProc(names[name.index as usize].clone())
                        }
                        NS_STATIC_FRAME => RelocTarget::StaticFrame,
                        NS_IMPORT_DATA => {
                            RelocTarget::ImportData(names[name.index as usize].clone())
                        }
                        ns => {
                            return Err(CodegenError::Backend {
                                proc: p.name.clone(),
                                msg: format!("relocation to namespace {ns}"),
                            });
                        }
                    };
                    let kind = match r.kind {
                        ClReloc::Abs8 => RelocKind::Abs8,
                        ClReloc::X86PCRel4 | ClReloc::X86CallPCRel4 => RelocKind::PcRel4,
                        other => {
                            return Err(CodegenError::Backend {
                                proc: p.name.clone(),
                                msg: format!("relocation kind {other:?}"),
                            });
                        }
                    };
                    relocs.push(Reloc {
                        offset: index32(at),
                        kind,
                        target,
                        addend: r.addend,
                    });
                }
                FinalizedRelocTarget::Func(off) => {
                    // A reference into this same function.
                    pending_local.push((at, r.kind, index32(pi), i64::from(*off) + r.addend));
                }
                FinalizedRelocTarget::ExternalName(other) => {
                    return Err(CodegenError::Backend {
                        proc: p.name.clone(),
                        msg: format!("relocation to {other:?}"),
                    });
                }
            }
        }
        let mut stmts: Vec<StmtRange> = compiled
            .buffer
            .get_srclocs_sorted()
            .iter()
            .filter(|s| s.loc.bits() != 0)
            .map(|s| StmtRange {
                stmt: s.loc.bits() - 1,
                start: index32(start) + s.start,
                end: index32(start) + s.end,
            })
            .collect();
        stmts.dedup();
        procs.push(ProcCode {
            name: p.name.clone(),
            index: index32(pi),
            offset: index32(start),
            size: index32(bytes.len()),
            export: p.export,
            entry: p.entry,
            stmts,
        });
    }
    // Local references: PC-relative to the procedure's start.
    for (at, kind, target, addend) in pending_local {
        let target_off = i64::from(procs[target as usize].offset);
        let disp = target_off + addend - i64::try_from(at).expect("code fits");
        match kind {
            ClReloc::X86PCRel4 | ClReloc::X86CallPCRel4 => {
                let d = i32::try_from(disp).map_err(|_| CodegenError::Backend {
                    proc: String::new(),
                    msg: "module code larger than 2 GiB".into(),
                })?;
                code[at..at + 4].copy_from_slice(&d.to_le_bytes());
            }
            ClReloc::Abs8 => {
                // Absolute: the loader adds the code base.
                relocs.push(Reloc {
                    offset: index32(at),
                    kind: RelocKind::Abs8,
                    target: RelocTarget::ImportProc(procs[target as usize].name.clone()),
                    addend,
                });
            }
            other => {
                return Err(CodegenError::Backend {
                    proc: String::new(),
                    msg: format!("local relocation kind {other:?}"),
                });
            }
        }
    }
    let static_init = layout.initial_bytes(m);
    let data_exports = m
        .vars
        .iter()
        .enumerate()
        .filter(|(_, v)| v.export)
        .filter_map(|(i, v)| {
            layout
                .module_var(index32(i))
                .map(|s| (v.name.clone(), s.offset))
        })
        .collect();
    Ok(ModuleCode {
        name: m.name.clone(),
        version: CODEGEN_VERSION,
        code,
        relocs,
        procs,
        statics: layout,
        static_init,
        data_exports,
    })
}

/// Native-order storage access, unaligned. Storage is big-endian, so
/// multi-byte integers are byte-swapped around every access; the x86
/// backend has no big-endian memory operations of its own.
pub(crate) fn le() -> MemFlagsData {
    MemFlagsData::new().with_notrap()
}

/// Flags for a bitcast between a float and its bits.
fn bits() -> MemFlagsData {
    MemFlagsData::new().with_endianness(Endianness::Little)
}

/// Lowering state for one procedure.
pub(crate) struct Lowerer<'a, 'b> {
    pub m: &'a Module,
    pub p: &'a Proc,
    pub pi: u32,
    pub b: FunctionBuilder<'b>,
    pub layout: &'a StaticLayout,
    names: &'a mut Vec<String>,
    pub vars: Vec<Variable>,
    pub blocks: Vec<Block>,
    pub slots: Vec<Option<StackSlot>>,
    pub env: Value,
    pub argc: Value,
    pub retbuf: Option<Value>,
    pub param_vals: Vec<Value>,
    pub unwind: Block,
    rt_refs: HashMap<RtFn, FuncRef>,
    local_refs: HashMap<u32, FuncRef>,
    import_refs: HashMap<String, FuncRef>,
    static_gv: Option<GlobalValue>,
    import_gvs: HashMap<String, GlobalValue>,
    /// Open `monitor` regions, innermost last.
    pub(crate) regions: Vec<storage::Region>,
    /// Regions whose dispatch blocks await filling.
    pub(crate) closed_regions: Vec<storage::Region>,
}

impl<'a, 'b> Lowerer<'a, 'b> {
    fn new(
        m: &'a Module,
        p: &'a Proc,
        pi: u32,
        b: FunctionBuilder<'b>,
        layout: &'a StaticLayout,
        names: &'a mut Vec<String>,
    ) -> Self {
        // Placeholders until the prologue runs.
        let zero = Value::from_u32(0);
        Self {
            m,
            p,
            pi,
            b,
            layout,
            names,
            vars: Vec::new(),
            blocks: Vec::new(),
            slots: Vec::new(),
            env: zero,
            argc: zero,
            retbuf: None,
            param_vals: Vec::new(),
            unwind: Block::from_u32(0),
            rt_refs: HashMap::new(),
            local_refs: HashMap::new(),
            import_refs: HashMap::new(),
            static_gv: None,
            import_gvs: HashMap::new(),
            regions: Vec::new(),
            closed_regions: Vec::new(),
        }
    }

    pub(crate) fn unsupported(&self, what: impl Into<String>) -> CodegenError {
        CodegenError::Unsupported {
            proc: self.p.name.clone(),
            what: what.into(),
        }
    }

    fn lower(&mut self) -> Result<()> {
        let p = self.p;
        for r in &p.regs {
            let v = self.b.declare_var(abi::reg_type(&r.ty));
            self.vars.push(v);
        }
        for _ in &p.blocks {
            let blk = self.b.create_block();
            self.blocks.push(blk);
        }
        self.unwind = self.b.create_block();
        self.prologue();
        for (bi, blk) in p.blocks.iter().enumerate() {
            self.b.switch_to_block(self.blocks[bi]);
            for i in &blk.insts {
                self.inst(i)?;
            }
        }
        self.fill_dispatch_blocks();
        self.epilogue_unwind();
        self.b.seal_all_blocks();
        Ok(())
    }

    /// The entry block: parameters, automatic storage, the call-stack entry.
    fn prologue(&mut self) {
        let entry = self.b.create_block();
        self.b.append_block_params_for_function_params(entry);
        self.b.switch_to_block(entry);
        let params: Vec<Value> = self.b.block_params(entry).to_vec();
        self.env = params[0];
        self.argc = params[1];
        let mut at = abi::HIDDEN_PARAMS;
        if !abi::returns_in_register(self.p.ret.as_ref()) {
            self.retbuf = Some(params[at]);
            at += 1;
        }
        self.param_vals = params[at..].to_vec();

        // Storage slots for automatics and by-value parameters.
        for v in &self.p.vars {
            let slot = match &v.class {
                StorageClass::Auto => Some(self.slot(&v.ty)),
                StorageClass::Param(k) => {
                    let param = &self.p.params[*k as usize];
                    if param.by_value {
                        Some(self.slot(&v.ty))
                    } else {
                        None
                    }
                }
                _ => None,
            };
            self.slots.push(slot);
        }
        for (i, v) in self.p.vars.iter().enumerate() {
            let Some(slot) = self.slots[i] else { continue };
            let addr = self.b.ins().stack_addr(types::I64, slot, 0);
            match &v.class {
                StorageClass::Auto => {
                    let bytes = v.inz.clone().unwrap_or_else(|| {
                        qir::inst::literal::default_bytes(&v.ty, usz(v.ty.size()), false)
                    });
                    self.store_bytes(addr, &bytes);
                }
                StorageClass::Param(k) => {
                    let k = *k as usize;
                    let raw = self.param_vals[k];
                    if v.ty.is_register() {
                        // Default when not passed, else the value.
                        let bytes =
                            qir::inst::literal::default_bytes(&v.ty, usz(v.ty.size()), false);
                        self.store_bytes(addr, &bytes);
                        let passed = self.b.ins().icmp_imm_s(
                            cranelift_codegen::ir::condcodes::IntCC::SignedGreaterThan,
                            self.argc,
                            i64::try_from(k).expect("few parameters"),
                        );
                        let store_blk = self.b.create_block();
                        let next = self.b.create_block();
                        self.b.ins().brif(passed, store_blk, &[], next, &[]);
                        self.b.switch_to_block(store_blk);
                        self.store_typed(addr, &v.ty, raw);
                        self.b.ins().jump(next, &[]);
                        self.b.switch_to_block(next);
                    } else {
                        let bytes =
                            qir::inst::literal::default_bytes(&v.ty, usz(v.ty.size()), false);
                        self.store_bytes(addr, &bytes);
                        let passed = self.b.ins().icmp_imm_s(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            raw,
                            0,
                        );
                        let copy_blk = self.b.create_block();
                        let next = self.b.create_block();
                        self.b.ins().brif(passed, copy_blk, &[], next, &[]);
                        self.b.switch_to_block(copy_blk);
                        self.copy(addr, raw, v.ty.size());
                        self.b.ins().jump(next, &[]);
                        self.b.switch_to_block(next);
                    }
                }
                _ => {}
            }
        }
        // The call-stack entry.
        let me = self.local_ref(self.pi);
        let code = self.b.ins().func_addr(types::I64, me);
        let enter = self.rt_ref(RtFn::Enter);
        self.b.ins().call(enter, &[self.env, code]);
        self.check_pending();
        self.b.ins().jump(self.blocks[0], &[]);
    }

    /// The unwind block: leave the call-stack entry and return zeros so
    /// the caller sees the pending exception.
    fn epilogue_unwind(&mut self) {
        self.b.switch_to_block(self.unwind);
        let leave = self.rt_ref(RtFn::Leave);
        self.b.ins().call(leave, &[self.env]);
        let rets: Vec<types::Type> = self
            .b
            .func
            .signature
            .returns
            .iter()
            .map(|r| r.value_type)
            .collect();
        let zeros: Vec<Value> = rets.iter().map(|t| self.zero(*t)).collect();
        self.b.ins().return_(&zeros);
    }

    pub(crate) fn zero(&mut self, t: types::Type) -> Value {
        if t == types::F32 {
            self.b.ins().f32const(0.0)
        } else if t == types::F64 {
            self.b.ins().f64const(0.0)
        } else if t == types::I128 {
            let lo = self.b.ins().iconst(types::I64, 0);
            let hi = self.b.ins().iconst(types::I64, 0);
            self.b.ins().iconcat(lo, hi)
        } else {
            self.b.ins().iconst(t, 0)
        }
    }

    fn slot(&mut self, ty: &Type) -> StackSlot {
        let size = u32::try_from(ty.size()).expect("frame fits");
        let align_shift = if ty.align() > 1 { 4 } else { 0 };
        self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            align_shift,
        ))
    }

    /// Poll the pending flag; branch to the unwind block when set.
    pub(crate) fn check_pending(&mut self) {
        let job = self.b.ins().load(types::I64, le(), self.env, abi::ENV_JOB);
        let pend = self.b.ins().load(types::I64, le(), job, abi::JOB_PENDING);
        let cont = self.b.create_block();
        let target = self.regions.last().map_or(self.unwind, |r| r.dispatch);
        self.b.ins().brif(pend, target, &[], cont, &[]);
        self.b.switch_to_block(cont);
    }

    /// Raise a condition and poll for resumption.
    pub(crate) fn raise(&mut self, cond: Cond) {
        let f = self.rt_ref(RtFn::Raise);
        let c = self.b.ins().iconst(types::I32, i64::from(cond as u32));
        self.b.ins().call(f, &[self.env, c]);
        self.check_pending();
    }

    /// Branch on `bad`: raise `cond` and continue with a zero of `ty`,
    /// or compute the value with `ok`.
    pub(crate) fn guard(
        &mut self,
        bad: Value,
        cond: Cond,
        ty: types::Type,
        ok: impl FnOnce(&mut Self) -> Value,
    ) -> Value {
        let raise_blk = self.b.create_block();
        let ok_blk = self.b.create_block();
        let merge = self.b.create_block();
        self.b.append_block_param(merge, ty);
        self.b.ins().brif(bad, raise_blk, &[], ok_blk, &[]);
        self.b.switch_to_block(raise_blk);
        self.raise(cond);
        let z = self.zero(ty);
        self.b.ins().jump(merge, &[z.into()]);
        self.b.switch_to_block(ok_blk);
        let v = ok(self);
        self.b.ins().jump(merge, &[v.into()]);
        self.b.switch_to_block(merge);
        self.b.block_params(merge)[0]
    }

    pub(crate) fn rt_ref(&mut self, f: RtFn) -> FuncRef {
        if let Some(r) = self.rt_refs.get(&f) {
            return *r;
        }
        let sig = self.b.import_signature(f.signature());
        let name = self
            .b
            .func
            .declare_imported_user_function(UserExternalName {
                namespace: NS_RT,
                index: f.index(),
            });
        let r = self.b.func.import_function(ExtFuncData {
            name: ExternalName::user(name),
            signature: sig,
            colocated: false,
            patchable: false,
        });
        self.rt_refs.insert(f, r);
        r
    }

    pub(crate) fn local_ref(&mut self, pi: u32) -> FuncRef {
        if let Some(r) = self.local_refs.get(&pi) {
            return *r;
        }
        let p = &self.m.procs[pi as usize];
        let sig = self
            .b
            .import_signature(abi::signature(&p.params, p.ret.as_ref()));
        let name = self
            .b
            .func
            .declare_imported_user_function(UserExternalName {
                namespace: NS_LOCAL,
                index: pi,
            });
        let r = self.b.func.import_function(ExtFuncData {
            name: ExternalName::user(name),
            signature: sig,
            colocated: true,
            patchable: false,
        });
        self.local_refs.insert(pi, r);
        r
    }

    fn name_index(&mut self, name: &str) -> u32 {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return index32(i);
        }
        self.names.push(name.to_string());
        index32(self.names.len() - 1)
    }

    pub(crate) fn import_ref(&mut self, proto: &qir::Proto) -> FuncRef {
        if let Some(r) = self.import_refs.get(&proto.name) {
            return *r;
        }
        let sig = self
            .b
            .import_signature(abi::signature(&proto.params, proto.ret.as_ref()));
        let index = self.name_index(&proto.name);
        let name = self
            .b
            .func
            .declare_imported_user_function(UserExternalName {
                namespace: NS_IMPORT_PROC,
                index,
            });
        let r = self.b.func.import_function(ExtFuncData {
            name: ExternalName::user(name),
            signature: sig,
            colocated: false,
            patchable: false,
        });
        self.import_refs.insert(proto.name.clone(), r);
        r
    }

    /// The program static area base plus this module's frame offset.
    pub(crate) fn static_base(&mut self) -> Value {
        let gv = if let Some(gv) = self.static_gv {
            gv
        } else {
            {
                let name = self
                    .b
                    .func
                    .declare_imported_user_function(UserExternalName {
                        namespace: NS_STATIC_FRAME,
                        index: 0,
                    });
                let gv = self.b.create_global_value(GlobalValueData::Symbol {
                    name: ExternalName::user(name),
                    offset: 0.into(),
                    colocated: false,
                    tls: false,
                });
                self.static_gv = Some(gv);
                gv
            }
        };
        let area = self
            .b
            .ins()
            .load(types::I64, le(), self.env, abi::ENV_STATICS);
        let frame = self.b.ins().symbol_value(types::I64, gv);
        self.b.ins().iadd(area, frame)
    }

    /// The address of a static another module exports.
    pub(crate) fn import_data(&mut self, name: &str) -> Value {
        let gv = if let Some(gv) = self.import_gvs.get(name) {
            *gv
        } else {
            {
                let index = self.name_index(name);
                let ext = self
                    .b
                    .func
                    .declare_imported_user_function(UserExternalName {
                        namespace: NS_IMPORT_DATA,
                        index,
                    });
                let gv = self.b.create_global_value(GlobalValueData::Symbol {
                    name: ExternalName::user(ext),
                    offset: 0.into(),
                    colocated: false,
                    tls: false,
                });
                self.import_gvs.insert(name.to_string(), gv);
                gv
            }
        };
        let area = self
            .b
            .ins()
            .load(types::I64, le(), self.env, abi::ENV_STATICS);
        let off = self.b.ins().symbol_value(types::I64, gv);
        self.b.ins().iadd(area, off)
    }

    /// Mark the statement the following code belongs to.
    pub(crate) fn stmt(&mut self, n: u32) {
        self.b.set_srcloc(SourceLoc::new(n + 1));
    }

    /// Store raw bytes at an address, in 8, 4, 2 and 1 byte pieces.
    pub(crate) fn store_bytes(&mut self, addr: Value, bytes: &[u8]) {
        let mut at = 0usize;
        while at < bytes.len() {
            let left = bytes.len() - at;
            let (ty, n) = if left >= 8 {
                (types::I64, 8)
            } else if left >= 4 {
                (types::I32, 4)
            } else if left >= 2 {
                (types::I16, 2)
            } else {
                (types::I8, 1)
            };
            let mut buf = [0u8; 8];
            buf[..n].copy_from_slice(&bytes[at..at + n]);
            #[allow(clippy::cast_possible_wrap)]
            let v = self.b.ins().iconst(ty, u64::from_le_bytes(buf) as i64);
            self.b.ins().store(le(), v, addr, off32(at as u64));
            at += n;
        }
    }

    /// Copy `len` bytes: inline for small sizes, the runtime otherwise.
    pub(crate) fn copy(&mut self, dst: Value, src: Value, len: u64) {
        if len <= 64 {
            let mut at = 0u64;
            while at < len {
                let left = len - at;
                let ty = if left >= 8 {
                    types::I64
                } else if left >= 4 {
                    types::I32
                } else if left >= 2 {
                    types::I16
                } else {
                    types::I8
                };
                let v = self.b.ins().load(ty, le(), src, off32(at));
                self.b.ins().store(le(), v, dst, off32(at));
                at += u64::from(ty.bytes());
            }
        } else {
            let f = self.rt_ref(RtFn::Copy);
            #[allow(clippy::cast_possible_wrap)]
            let n = self.b.ins().iconst(types::I64, len as i64);
            self.b.ins().call(f, &[dst, src, n]);
        }
    }

    /// Load a big-endian integer of a Cranelift type.
    pub(crate) fn load_be(&mut self, ty: types::Type, addr: Value) -> Value {
        let v = self.b.ins().load(ty, le(), addr, 0);
        if ty.bits() > 8 {
            self.b.ins().bswap(v)
        } else {
            v
        }
    }

    /// Store an integer big-endian.
    pub(crate) fn store_be(&mut self, v: Value, addr: Value) {
        let ty = self.b.func.dfg.value_type(v);
        let v = if ty.bits() > 8 {
            self.b.ins().bswap(v)
        } else {
            v
        };
        self.b.ins().store(le(), v, addr, 0);
    }

    /// Load a register-typed value from storage.
    pub(crate) fn load_typed(&mut self, addr: Value, ty: &Type) -> Value {
        match ty {
            Type::Ind => {
                let v = self.b.ins().load(types::I8, le(), addr, 0);
                self.b
                    .ins()
                    .icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::Equal, v, 0xF1)
            }
            Type::F32 => {
                let v = self.load_be(types::I32, addr);
                self.b.ins().bitcast(types::F32, bits(), v)
            }
            Type::F64 => {
                let v = self.load_be(types::I64, addr);
                self.b.ins().bitcast(types::F64, bits(), v)
            }
            Type::Ptr | Type::ProcPtr => self.b.ins().load(types::I128, le(), addr, 0),
            _ => self.load_be(abi::reg_type(ty), addr),
        }
    }

    /// Store a register-typed value to storage.
    pub(crate) fn store_typed(&mut self, addr: Value, ty: &Type, v: Value) {
        match ty {
            Type::Ind => {
                let on = self.b.ins().iconst(types::I8, 0xF1);
                let off = self.b.ins().iconst(types::I8, 0xF0);
                let byte = self.b.ins().select(v, on, off);
                self.b.ins().store(le(), byte, addr, 0);
            }
            Type::F32 => {
                let raw = self.b.ins().bitcast(types::I32, bits(), v);
                self.store_be(raw, addr);
            }
            Type::F64 => {
                let raw = self.b.ins().bitcast(types::I64, bits(), v);
                self.store_be(raw, addr);
            }
            Type::Ptr | Type::ProcPtr => {
                self.b.ins().store(le(), v, addr, 0);
            }
            _ => self.store_be(v, addr),
        }
    }

    /// A literal as a register value of a type.
    pub(crate) fn const_value(&mut self, c: &qir::Const, ty: &Type) -> Result<Value> {
        let bytes = qir::inst::literal::encode(ty, c).map_err(|e| self.unsupported(e))?;
        Ok(match ty {
            Type::Ind => self.b.ins().iconst(types::I8, i64::from(bytes[0] == 0xF1)),
            Type::I8 => self
                .b
                .ins()
                .iconst(types::I8, i64::from(i8::from_be_bytes([bytes[0]]))),
            Type::U8 => self.b.ins().iconst(types::I8, i64::from(bytes[0])),
            Type::I16 => self.b.ins().iconst(
                types::I16,
                i64::from(i16::from_be_bytes([bytes[0], bytes[1]])),
            ),
            Type::U16 => self.b.ins().iconst(
                types::I16,
                i64::from(u16::from_be_bytes([bytes[0], bytes[1]])),
            ),
            Type::I32 => self.b.ins().iconst(
                types::I32,
                i64::from(i32::from_be_bytes(bytes[..4].try_into().expect("4"))),
            ),
            Type::U32 => self.b.ins().iconst(
                types::I32,
                i64::from(u32::from_be_bytes(bytes[..4].try_into().expect("4"))),
            ),
            Type::I64 | Type::U64 => self.b.ins().iconst(
                types::I64,
                i64::from_be_bytes(bytes[..8].try_into().expect("8")),
            ),
            Type::F32 => self
                .b
                .ins()
                .f32const(f32::from_be_bytes(bytes[..4].try_into().expect("4"))),
            Type::F64 => self
                .b
                .ins()
                .f64const(f64::from_be_bytes(bytes[..8].try_into().expect("8"))),
            Type::Ptr | Type::ProcPtr => {
                let lo = i64::from_le_bytes(bytes[..8].try_into().expect("8"));
                let hi = i64::from_le_bytes(bytes[8..16].try_into().expect("8"));
                let lo = self.b.ins().iconst(types::I64, lo);
                let hi = self.b.ins().iconst(types::I64, hi);
                self.b.ins().iconcat(lo, hi)
            }
            _ => return Err(self.unsupported(format!("a {ty} literal in a register"))),
        })
    }
}
