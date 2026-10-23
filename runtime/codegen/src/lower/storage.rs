// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage instructions: decimal, conversion, string, date and
//! pointer operations lower to runtime calls with (address, descriptor)
//! operand pairs; `monitor` regions become dispatch blocks the pending
//! poll branches to.

use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::{Block, StackSlotData, StackSlotKind, Value, types};
use qir::verify::typing::operand_type;
use qir::{
    Callee, Const, CvtOpts, DecUn, DtInst, DtaqInst, Handler, Inst, IoInst, MsgInst, MsgQueue,
    Operand, Place, PtrInst, Selector, StrInst, TrimSide, Type, TypeClass,
};

use super::Lowerer;
use crate::abi::{self, TypeDesc};
use crate::error::Result;
use crate::rt::{
    RtFn, cvt_opts, db_op_code, dec_flags, dev_op_code, dtaara_op_code, pack_msgid, selector,
};

/// An open `monitor` region while its blocks are lowered.
pub(crate) struct Region {
    /// Number in the module's region table, from 1.
    pub id: u32,
    /// The block the pending poll branches to inside the region.
    pub dispatch: Block,
    /// Handlers in order.
    pub handlers: Vec<Handler>,
    /// The enclosing region's dispatch block, or none.
    pub parent: Option<Block>,
}

impl Lowerer<'_, '_> {
    /// Lower a storage instruction.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn ext_inst(&mut self, i: &Inst) -> Result<()> {
        match i {
            Inst::DecBin {
                dst,
                op,
                a,
                b,
                prec,
                halfadj,
                resdecpos,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (aa, ad) = self.sop(a, None)?;
                let (ba, bd) = self.sop(b, None)?;
                let opv = self.i64(i64::from(*op as u8));
                let precv = self.i64(i64::from(prec.digits) | (i64::from(prec.scale) << 8));
                let mut flags = 0;
                if *halfadj {
                    flags |= dec_flags::HALFADJ;
                }
                if *resdecpos {
                    flags |= dec_flags::RESDECPOS;
                }
                let fl = self.i64(flags);
                self.rt_call(RtFn::DecBin, &[opv, da, dd, aa, ad, ba, bd, precv, fl]);
            }
            Inst::DecUn {
                dst,
                op,
                src,
                halfadj,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let opv = self.i64(match op {
                    DecUn::Mov => 0,
                    DecUn::Neg => 1,
                    DecUn::Abs => 2,
                });
                let h = self.i64(i64::from(*halfadj));
                self.rt_call(RtFn::DecUn, &[opv, da, dd, sa, sd, h]);
            }
            Inst::DecCmp { dst, cond, a, b } => {
                let (aa, ad) = self.sop(a, None)?;
                let (ba, bd) = self.sop(b, None)?;
                let c = self
                    .rt_call(RtFn::DecCmp, &[aa, ad, ba, bd])
                    .expect("result");
                let v = self
                    .b
                    .ins()
                    .icmp_imm_s(super::inst::int_cc(*cond, true), c, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Convert { dst, src, opts } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let o = self.i64(pack_cvt_opts(opts));
                self.rt_call(RtFn::Convert, &[da, dd, sa, sd, o]);
            }
            Inst::Str(s) => self.str_inst(s)?,
            Inst::Dt(d) => self.dt_inst(d)?,
            Inst::Ptr(p) => self.ptr_inst(p)?,
            Inst::Monitor { handlers, .. } => {
                let dispatch = self.b.create_block();
                let parent = self.regions.last().map(|r| r.dispatch);
                let parent_id = self.regions.last().map_or(0, |r| r.id);
                self.region_table.push(crate::output::RegionDesc {
                    handlers: handlers
                        .iter()
                        .map(|h| encode_selector(&h.selector))
                        .collect(),
                    parent: parent_id,
                });
                let id = u32::try_from(self.region_table.len()).expect("few regions");
                self.regions.push(Region {
                    id,
                    dispatch,
                    handlers: handlers.clone(),
                    parent,
                });
            }
            Inst::EndMon(_) => {
                let r = self.regions.pop().expect("verified nesting");
                self.closed_regions.push(r);
            }
            Inst::Alloc { dst, n } => {
                let nv = self.int_op(n)?;
                let out = self.ptr_slot();
                self.rt_call(RtFn::Alloc, &[out, nv]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Realloc { dst, ptr, n } => {
                let (pa, _) = self.sop(ptr, Some(&Type::Ptr))?;
                let nv = self.int_op(n)?;
                let out = self.ptr_slot();
                self.rt_call(RtFn::Realloc, &[out, pa, nv]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Dealloc { ptr } => {
                let (pa, _) = self.sop(ptr, Some(&Type::Ptr))?;
                self.rt_call(RtFn::Dealloc, &[pa]);
            }
            Inst::Msg(m) => self.msg_inst(m)?,
            Inst::Io(io) => self.io_inst(io)?,
            Inst::Dtaara {
                op,
                name,
                lib,
                data,
            } => {
                let opv = self.i64(dtaara_op_code(*op));
                let (na, _) = self.sop(name, Some(&Type::Char(10)))?;
                let (la, _) = self.sop(lib, Some(&Type::Char(10)))?;
                let (da, dd) = match data {
                    Some(p) => self.place_pair(p)?,
                    None => (self.i64(0), self.i64(0)),
                };
                self.rt_call(RtFn::Dtaara, &[opv, na, la, da, dd]);
            }
            Inst::Dtaq(DtaqInst::Snd {
                name,
                lib,
                data,
                len,
            }) => {
                let (na, _) = self.sop(name, Some(&Type::Char(10)))?;
                let (la, _) = self.sop(lib, Some(&Type::Char(10)))?;
                let (da, _) = self.sop(data, None)?;
                let ln = self.int_op(len)?;
                self.rt_call(RtFn::DtaqSnd, &[na, la, da, ln]);
            }
            Inst::Dtaq(DtaqInst::Rcv {
                dst,
                name,
                lib,
                data,
                len,
                wait,
            }) => {
                let (na, _) = self.sop(name, Some(&Type::Char(10)))?;
                let (la, _) = self.sop(lib, Some(&Type::Char(10)))?;
                let (da, _) = self.place_pair(data)?;
                let ln = self.int_op(len)?;
                let w = self.int_op(wait)?;
                let v = self
                    .rt_call(RtFn::DtaqRcv, &[na, la, da, ln, w])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            Inst::Dsply { src, reply } => {
                let (sa, sd) = self.sop(src, None)?;
                let (ra, rd) = match reply {
                    Some(p) => self.place_pair(p)?,
                    None => (self.i64(0), self.i64(0)),
                };
                self.rt_call(RtFn::Dsply, &[sa, sd, ra, rd]);
            }
            other => {
                let name = format!("{other:?}");
                let name = name.split(['{', '(', ' ']).next().unwrap_or("instruction");
                return Err(self.unsupported(name));
            }
        }
        Ok(())
    }

    /// The kind and count words of a message queue reference.
    fn queue_ref(&mut self, q: &MsgQueue) -> Result<(Value, Value)> {
        let (kind, n) = match q {
            MsgQueue::Same => (0, None),
            MsgQueue::Prv => (1, None),
            MsgQueue::CtlBdy => (2, None),
            MsgQueue::Ext => (3, None),
            MsgQueue::SysOpr => (4, None),
            MsgQueue::Up(o) => (5, Some(o)),
        };
        let k = self.i64(kind);
        let n = match n {
            Some(o) => self.int_op(o)?,
            None => self.i64(0),
        };
        Ok((k, n))
    }

    /// A place as an address, or 0 when absent.
    fn opt_place_addr(&mut self, p: Option<&Place>) -> Result<Value> {
        match p {
            Some(p) => Ok(self.addr(p)?.0),
            None => Ok(self.i64(0)),
        }
    }

    /// A place as an (address, descriptor) pair, or zeros when absent.
    fn opt_place_pair(&mut self, p: Option<&Place>) -> Result<(Value, Value)> {
        match p {
            Some(p) => self.place_pair(p),
            None => Ok((self.i64(0), self.i64(0))),
        }
    }

    fn msg_inst(&mut self, m: &MsgInst) -> Result<()> {
        match m {
            MsgInst::Snd {
                msgid,
                msgf,
                msgf_lib,
                data,
                msg_type,
                to,
                key,
            } => {
                let (ia, _) = self.sop(msgid, Some(&Type::Char(7)))?;
                let (fa, _) = self.sop(msgf, Some(&Type::Char(10)))?;
                let (la, _) = match msgf_lib {
                    Some(l) => self.sop(l, Some(&Type::Char(10)))?,
                    None => (self.i64(0), self.i64(0)),
                };
                let (da, dd) = self.sop_opt(data.as_ref())?;
                let ty = self.i64(i64::from(*msg_type as u8));
                let (tk, tn) = self.queue_ref(to)?;
                let ka = self.opt_place_addr(key.as_ref())?;
                self.rt_call(RtFn::SndPgmMsg, &[ia, fa, la, da, dd, ty, tk, tn, ka]);
            }
            MsgInst::Rcv {
                dst,
                pgmq,
                msg_type,
                key,
                rmv,
                msg,
                msgid,
                data,
                keyout,
                sev,
            } => {
                let (qk, qn) = self.queue_ref(pgmq)?;
                let ty = self.i64(msg_type.map_or(-1, |t| i64::from(t as u8)));
                let (ka, _) = self.sop_opt(key.as_ref())?;
                let rm = self.i64(i64::from(*rmv));
                let (ma, mt) = self.opt_place_pair(msg.as_ref())?;
                let ida = self.opt_place_addr(msgid.as_ref())?;
                let (da, dd) = self.opt_place_pair(data.as_ref())?;
                let ko = self.opt_place_addr(keyout.as_ref())?;
                let (sa, st) = self.opt_place_pair(sev.as_ref())?;
                let v = self
                    .rt_call(
                        RtFn::RcvMsg,
                        &[qk, qn, ty, ka, rm, ma, mt, ida, da, dd, ko, sa, st],
                    )
                    .expect("result");
                let ind = self.b.ins().ireduce(types::I8, v);
                self.b.def_var(self.vars[dst.0 as usize], ind);
            }
            MsgInst::Rmv { pgmq, key, clear } => {
                let (qk, qn) = self.queue_ref(pgmq)?;
                let (ka, _) = self.sop_opt(key.as_ref())?;
                let c = self.i64(match clear.as_str() {
                    "*OLD" => 1,
                    "*NEW" => 2,
                    "*KEEPUNANS" => 3,
                    _ => 0,
                });
                self.rt_call(RtFn::RmvMsg, &[qk, qn, ka, c]);
            }
        }
        Ok(())
    }

    /// Lower a file I/O instruction: every operation yields its status.
    fn io_inst(&mut self, io: &IoInst) -> Result<()> {
        match io {
            IoInst::DbOpen { dst, file, infds } => {
                let f = self.i64(i64::from(*file));
                let (ia, id) = match infds {
                    Some(p) => self.place_pair(p)?,
                    None => (self.i64(0), self.i64(0)),
                };
                let v = self.rt_call(RtFn::DbOpen, &[f, ia, id]).expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            IoInst::DbClose { dst, file } => {
                let f = self.i64(i64::from(*file));
                let v = self.rt_call(RtFn::DbClose, &[f]).expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            IoInst::DbIo {
                dst,
                file,
                op,
                rec,
                key,
                rrn,
                fmt,
                nolock,
            } => {
                let f = self.i64(i64::from(*file));
                let opv = self.i64(db_op_code(*op));
                let (ra, rd) = match rec {
                    Some(p) => self.place_pair(p)?,
                    None => (self.i64(0), self.i64(0)),
                };
                let (ka, kd) = match key {
                    Some(k) => self.sop(k, None)?,
                    None => (self.i64(0), self.i64(0)),
                };
                let rr = self.int_opt(rrn.as_ref(), 0)?;
                let (fa, _) = match fmt {
                    Some(n) => self.sop(
                        &Operand::Const(Const::Str(n.clone())),
                        Some(&Type::Char(10)),
                    )?,
                    None => (self.i64(0), self.i64(0)),
                };
                let nl = self.i64(i64::from(*nolock));
                let v = self
                    .rt_call(RtFn::DbIo, &[f, opv, ra, rd, ka, kd, rr, fa, nl])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            IoInst::DevIo {
                dst,
                file,
                op,
                fmt,
                rec,
            } => {
                let f = self.i64(i64::from(*file));
                let opv = self.i64(dev_op_code(*op));
                let (fa, _) = self.sop(
                    &Operand::Const(Const::Str(fmt.clone())),
                    Some(&Type::Char(10)),
                )?;
                let (ra, rd) = match rec {
                    Some(p) => self.place_pair(p)?,
                    None => (self.i64(0), self.i64(0)),
                };
                let v = self
                    .rt_call(RtFn::DevIo, &[f, opv, fa, ra, rd])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
        }
        Ok(())
    }

    /// Fill every region's dispatch block: try its handlers in order,
    /// then fall through to the enclosing region or the unwind block.
    pub(crate) fn fill_dispatch_blocks(&mut self) {
        let regions = std::mem::take(&mut self.closed_regions);
        for r in &regions {
            self.b.switch_to_block(r.dispatch);
            for h in &r.handlers {
                let (kind, a, b) = encode_selector(&h.selector);
                let kv = self.i64(kind);
                let av = self.i64(a);
                let bv = self.i64(b);
                let f = self.rt_ref(RtFn::MonitorMatch);
                let call = self.b.ins().call(f, &[self.env, kv, av, bv]);
                let m = self.b.inst_results(call)[0];
                let next = self.b.create_block();
                self.b
                    .ins()
                    .brif(m, self.blocks[h.target.0 as usize], &[], next, &[]);
                self.b.switch_to_block(next);
            }
            let fallthrough = r.parent.unwrap_or(self.unwind);
            self.b.ins().jump(fallthrough, &[]);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn str_inst(&mut self, s: &StrInst) -> Result<()> {
        match s {
            StrInst::Cat { dst, a, b } => {
                let (da, dd) = self.place_pair(dst)?;
                let (aa, ad) = self.sop(a, None)?;
                let (ba, bd) = self.sop(b, None)?;
                self.rt_call(RtFn::StrCat, &[da, dd, aa, ad, ba, bd]);
            }
            StrInst::Subst {
                dst,
                src,
                start,
                len,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let st = self.int_op(start)?;
                let ln = self.int_op(len)?;
                self.rt_call(RtFn::StrSubst, &[da, dd, sa, sd, st, ln]);
            }
            StrInst::Scan {
                dst,
                needle,
                hay,
                start,
            } => {
                let (na, nd) = self.sop(needle, None)?;
                let (ha, hd) = self.sop(hay, None)?;
                let st = self.int_opt(start.as_ref(), 0)?;
                let v = self
                    .rt_call(RtFn::StrScan, &[na, nd, ha, hd, st])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            StrInst::Check {
                dst,
                set,
                src,
                start,
                reverse,
            } => {
                let (sa, sd) = self.sop(set, None)?;
                let (ca, cd) = self.sop(src, None)?;
                let st = self.int_opt(start.as_ref(), 0)?;
                let rv = self.i64(i64::from(*reverse));
                let v = self
                    .rt_call(RtFn::StrCheck, &[sa, sd, ca, cd, st, rv])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            StrInst::Xlate {
                dst,
                from,
                to,
                src,
                start,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (fa, fd) = self.sop(from, None)?;
                let (ta, td) = self.sop(to, None)?;
                let (sa, sd) = self.sop(src, None)?;
                let st = self.int_opt(start.as_ref(), 0)?;
                self.rt_call(RtFn::StrXlate, &[da, dd, fa, fd, ta, td, sa, sd, st]);
            }
            StrInst::Trim {
                dst,
                src,
                chars,
                side,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let (ca, cd) = self.sop_opt(chars.as_ref())?;
                let sv = self.i64(match side {
                    TrimSide::Both => 0,
                    TrimSide::Left => 1,
                    TrimSide::Right => 2,
                });
                self.rt_call(RtFn::StrTrim, &[da, dd, sa, sd, ca, cd, sv]);
            }
            StrInst::Cmp { dst, cond, a, b } => {
                let (aa, ad) = self.sop(a, None)?;
                let (ba, bd) = self.sop(b, None)?;
                let c = self
                    .rt_call(RtFn::StrCmp, &[aa, ad, ba, bd])
                    .expect("result");
                let v = self
                    .b
                    .ins()
                    .icmp_imm_s(super::inst::int_cc(*cond, true), c, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            StrInst::Len { dst, src } => {
                let (sa, sd) = self.sop(src, None)?;
                let v = self.rt_call(RtFn::StrLen, &[sa, sd]).expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            StrInst::SetLen { dst, n } => {
                let (da, dd) = self.place_pair(dst)?;
                let nv = self.int_op(n)?;
                self.rt_call(RtFn::StrSetLen, &[da, dd, nv]);
            }
            StrInst::Replace {
                dst,
                src,
                repl,
                start,
                len,
            } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let (ra, rd) = self.sop(repl, None)?;
                let st = self.int_op(start)?;
                let ln = self.int_opt(len.as_ref(), -1)?;
                self.rt_call(RtFn::StrReplace, &[da, dd, sa, sd, ra, rd, st, ln]);
            }
        }
        Ok(())
    }

    fn dt_inst(&mut self, d: &DtInst) -> Result<()> {
        match d {
            DtInst::Add { dst, src, n, unit } => {
                let (da, dd) = self.place_pair(dst)?;
                let (sa, sd) = self.sop(src, None)?;
                let nv = self.int_op(n)?;
                let u = self.i64(i64::from(*unit as u8));
                self.rt_call(RtFn::DtAdd, &[da, dd, sa, sd, nv, u]);
            }
            DtInst::Diff { dst, a, b, unit } => {
                let (aa, ad) = self.sop(a, None)?;
                let (ba, bd) = self.sop(b, None)?;
                let u = self.i64(i64::from(*unit as u8));
                let v = self
                    .rt_call(RtFn::DtDiff, &[aa, ad, ba, bd, u])
                    .expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            DtInst::Extract { dst, src, unit } => {
                let (sa, sd) = self.sop(src, None)?;
                let u = self.i64(i64::from(*unit as u8));
                let v = self.rt_call(RtFn::DtExtract, &[sa, sd, u]).expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            DtInst::Test {
                dst,
                src,
                datfmt,
                timfmt,
            } => {
                let (sa, sd) = self.sop(src, None)?;
                let df = self.i64(datfmt.map_or(0, |f| i64::from(abi::date_fmt_index(f)) + 1));
                let tf = self.i64(timfmt.map_or(0, |f| i64::from(abi::time_fmt_index(f)) + 1));
                let v = self
                    .rt_call(RtFn::DtTest, &[sa, sd, df, tf])
                    .expect("result");
                let ind = self.b.ins().ireduce(types::I8, v);
                self.b.def_var(self.vars[dst.0 as usize], ind);
            }
            DtInst::Now { dst } => {
                let (da, dd) = self.place_pair(dst)?;
                self.rt_call(RtFn::DtNow, &[da, dd]);
            }
        }
        Ok(())
    }

    fn ptr_inst(&mut self, p: &PtrInst) -> Result<()> {
        match p {
            PtrInst::Addr { dst, place } => {
                let (a, ty) = self.addr(place)?;
                let out = self.ptr_slot();
                let len = self.i64(i64::try_from(ty.size()).expect("size fits"));
                let frame = self.i64(i64::from(self.in_frame(place)));
                self.rt_call(RtFn::PtrAddr, &[out, a, len, frame]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            PtrInst::PtrAdd { dst, ptr, n } => {
                let (pa, _) = self.sop(ptr, Some(&Type::Ptr))?;
                let nv = self.int_op(n)?;
                let out = self.ptr_slot();
                self.rt_call(RtFn::PtrAdd, &[out, pa, nv]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            PtrInst::PtrDiff { dst, p, q } => {
                let (pa, _) = self.sop(p, Some(&Type::Ptr))?;
                let (qa, _) = self.sop(q, Some(&Type::Ptr))?;
                let v = self.rt_call(RtFn::PtrDiff, &[pa, qa]).expect("result");
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            PtrInst::ProcAddr { dst, callee } => {
                let fref = match callee {
                    Callee::Local(r) => self.local_ref(r.0),
                    Callee::Import(r) => {
                        let proto = self.m.protos[r.0 as usize].clone();
                        self.import_ref(&proto)
                    }
                };
                let code = self.b.ins().func_addr(types::I64, fref);
                let out = self.ptr_slot();
                self.rt_call(RtFn::ProcAddr, &[out, code]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
            PtrInst::Rslvsp {
                dst,
                obj_type,
                name,
                lib,
            } => {
                let (oa, _) = self.sop(
                    &Operand::Const(Const::Str(obj_type.clone())),
                    Some(&Type::Char(10)),
                )?;
                let (na, _) = self.sop(name, Some(&Type::Char(10)))?;
                let (la, _) = self.sop(lib, Some(&Type::Char(10)))?;
                let out = self.ptr_slot();
                self.rt_call(RtFn::Rslvsp, &[out, oa, na, la]);
                let v = self.b.ins().load(types::I128, super::le(), out, 0);
                self.b.def_var(self.vars[dst.0 as usize], v);
            }
        }
        Ok(())
    }

    /// `cvt.T` of a numeric place: through the runtime.
    pub(crate) fn cvt_from_storage(
        &mut self,
        place: &Place,
        dty: &Type,
        chk: bool,
    ) -> Result<Value> {
        let (sa, sd) = self.place_pair(place)?;
        let dcl = abi::reg_type(dty);
        Ok(if dty.class() == TypeClass::Float {
            let f = self.rt_call(RtFn::DecToFloat, &[sa, sd]).expect("result");
            if dcl == types::F32 {
                self.b.ins().fdemote(types::F32, f)
            } else {
                f
            }
        } else {
            let bits = self.i64(i64::from(dcl.bits()));
            let signed = self.i64(i64::from(dty.class() == TypeClass::Int));
            let c = self.i64(i64::from(chk));
            let v = self
                .rt_call(RtFn::DecToInt, &[sa, sd, bits, signed, c])
                .expect("result");
            if dcl == types::I64 {
                v
            } else {
                self.b.ins().ireduce(dcl, v)
            }
        })
    }

    /// Call a runtime function with `env` first and poll for an
    /// exception afterwards. Returns the first result.
    pub(crate) fn rt_call(&mut self, f: RtFn, args: &[Value]) -> Option<Value> {
        let fref = self.rt_ref(f);
        let mut vals = Vec::with_capacity(args.len() + 1);
        vals.push(self.env);
        vals.extend_from_slice(args);
        self.mark_region();
        let call = self.b.ins().call(fref, &vals);
        let r = self.b.inst_results(call).first().copied();
        self.check_pending();
        r
    }

    pub(crate) fn i64(&mut self, v: i64) -> Value {
        self.b.ins().iconst(types::I64, v)
    }

    fn desc(&mut self, ty: &Type) -> Value {
        self.i64(TypeDesc::of(ty).word())
    }

    /// A place as an (address, descriptor) pair.
    pub(crate) fn place_pair(&mut self, p: &Place) -> Result<(Value, Value)> {
        let (a, ty) = self.addr(p)?;
        let d = self.desc(&ty);
        Ok((a, d))
    }

    /// A 16-byte aligned temporary for a pointer result.
    fn ptr_slot(&mut self) -> Value {
        let slot =
            self.b
                .create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 4));
        self.b.ins().stack_addr(types::I64, slot, 0)
    }

    /// A temporary holding bytes, as an (address, descriptor) pair.
    fn temp_bytes(&mut self, ty: &Type, bytes: &[u8]) -> (Value, Value) {
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            u32::try_from(bytes.len().max(1)).expect("literal fits"),
            if ty.align() > 1 { 4 } else { 0 },
        ));
        let a = self.b.ins().stack_addr(types::I64, slot, 0);
        self.store_bytes(a, bytes);
        let d = self.desc(ty);
        (a, d)
    }

    /// A storage operand: a place as it is, a register spilled to a
    /// temporary of its type, a literal encoded into a temporary of a
    /// type that holds it exactly.
    pub(crate) fn sop(&mut self, op: &Operand, hint: Option<&Type>) -> Result<(Value, Value)> {
        match op {
            Operand::Place(p) => self.place_pair(p),
            Operand::Reg(r) => {
                let ty = self.p.reg_type(*r).expect("typed").clone();
                let v = self.b.use_var(self.vars[r.0 as usize]);
                let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    u32::try_from(ty.size()).expect("small"),
                    if ty.align() > 1 { 4 } else { 0 },
                ));
                let a = self.b.ins().stack_addr(types::I64, slot, 0);
                self.store_typed(a, &ty, v);
                let d = self.desc(&ty);
                Ok((a, d))
            }
            Operand::Const(c) => {
                let ty = match (c, hint) {
                    (Const::Int(v), _) => {
                        let digits = v.unsigned_abs().to_string().len().max(1);
                        Type::Packed(
                            qir::Shape::new(u8::try_from(digits).expect("≤ 39"), 0).expect("valid"),
                        )
                    }
                    (Const::Dec(s), _) => {
                        let d = qdecimal::Decimal::parse(s)
                            .map_err(|e| self.unsupported(e.to_string()))?;
                        let sh = d.shape();
                        Type::Packed(qir::Shape::new(sh.digits.max(1), sh.scale).expect("valid"))
                    }
                    (Const::Float(_), _) => Type::F64,
                    (Const::Str(s), Some(t @ Type::Char(n)))
                        if s.chars().count() <= *n as usize =>
                    {
                        t.clone()
                    }
                    (Const::Str(s), _) => {
                        let n = u32::try_from(s.chars().count().max(1)).expect("literal fits");
                        Type::Char(n)
                    }
                    (Const::Hex(h), _) => {
                        Type::Char(u32::try_from(h.len().max(1)).expect("literal fits"))
                    }
                    (Const::On | Const::Off, _) => Type::Ind,
                    (Const::Null, _) => Type::Ptr,
                    (Const::Blanks, Some(t)) if t.is_string() => t.clone(),
                    (Const::Blanks, _) => Type::Char(1),
                    (Const::Zeros | Const::HiVal | Const::LoVal, Some(t)) => t.clone(),
                    (Const::Zeros, None) => Type::Packed(qir::Shape::new(1, 0).expect("valid")),
                    (Const::HiVal | Const::LoVal, None) => {
                        return Err(self.unsupported("*HIVAL or *LOVAL without a typed operand"));
                    }
                };
                let bytes = qir::inst::literal::encode(&ty, c).map_err(|e| self.unsupported(e))?;
                Ok(self.temp_bytes(&ty, &bytes))
            }
        }
    }

    fn sop_opt(&mut self, op: Option<&Operand>) -> Result<(Value, Value)> {
        match op {
            Some(o) => self.sop(o, None),
            None => Ok((self.i64(0), self.i64(0))),
        }
    }

    /// An integer operand as an `i64`.
    pub(crate) fn int_op(&mut self, op: &Operand) -> Result<Value> {
        let ty =
            operand_type(self.m, self.p, op, Some(&Type::I64)).map_err(|e| self.unsupported(e))?;
        let v = self.value(op, &ty)?;
        Ok(self.extend_i64(v, &ty))
    }

    fn int_opt(&mut self, op: Option<&Operand>, absent: i64) -> Result<Value> {
        match op {
            Some(o) => self.int_op(o),
            None => Ok(self.i64(absent)),
        }
    }
}

/// A selector as the three words `monitor_match` takes.
fn encode_selector(s: &Selector) -> (i64, i64, i64) {
    match s {
        Selector::All => (selector::ALL, 0, 0),
        Selector::Program => (selector::PROGRAM, 0, 0),
        Selector::File => (selector::FILE, 0, 0),
        Selector::Status(a, b) => (selector::STATUS, i64::from(*a), i64::from(*b)),
        Selector::MsgId(id) => (selector::MSGID, pack_msgid(id), 0),
    }
}

/// The `opts` word of `convert`.
fn pack_cvt_opts(o: &CvtOpts) -> i64 {
    let mut w = 0i64;
    if o.halfadj {
        w |= cvt_opts::HALFADJ;
    }
    if let Some(d) = o.datfmt {
        w |= (i64::from(abi::date_fmt_index(d)) + 1) << cvt_opts::DATFMT_SHIFT;
    }
    if let Some(t) = o.timfmt {
        w |= (i64::from(abi::time_fmt_index(t)) + 1) << cvt_opts::TIMFMT_SHIFT;
    }
    if let Some(c) = o.ccsid {
        w |= i64::from(c) << cvt_opts::CCSID_SHIFT;
    }
    w
}
