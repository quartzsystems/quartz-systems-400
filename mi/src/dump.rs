// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job dumps: the exceptions raised, the call stack with statements,
//! each entry's messages and, for a full dump, the static storage of
//! every program on the stack, variable by variable.

use std::fmt::Write as _;

use crate::job::Job;

impl Job {
    /// The dump text; `full` adds static storage.
    #[must_use]
    pub fn dump(&self, full: bool) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Job dump . . . . . . . . :   {}",
            if full { "*FULL" } else { "*CALLSTK" }
        );
        let _ = writeln!(out, "Exceptions raised:");
        for e in &self.raised {
            let _ = writeln!(
                out,
                "  {:<7} {:<10} depth {:<3} status {:<5} {}",
                e.msgid,
                e.msgf,
                e.depth,
                e.status,
                ccsid::decode_trimmed(&e.data)
            );
        }
        let _ = writeln!(out, "Call stack:");
        let _ = writeln!(out, "  Depth Program    Procedure  Stmt   Region");
        let top = self.act.frames.len();
        for (i, f) in self.act.frames.iter().enumerate() {
            let pgm = f
                .program
                .as_ref()
                .map(|p| p.object.name.clone())
                .unwrap_or_default();
            let proc_name = f
                .program
                .as_ref()
                .and_then(|p| p.proc_name(f.code))
                .unwrap_or_default();
            let (stmt, region) = if i + 1 == top {
                (self.abi.cur_stmt, self.abi.cur_region)
            } else {
                (f.stmt, f.region)
            };
            let _ = writeln!(
                out,
                "  {:<5} {pgm:<10} {proc_name:<10} {stmt:<6} {region}{}",
                i + 1,
                if f.control_boundary { "  *CTLBDY" } else { "" }
            );
            for m in &f.msgq.msgs {
                let _ = writeln!(
                    out,
                    "        {:<7} {:<8} {}",
                    m.msgid,
                    m.msg_type.name(),
                    m.text
                );
            }
        }
        if full {
            let mut seen: Vec<String> = Vec::new();
            for f in &self.act.frames {
                let Some(p) = &f.program else {
                    continue;
                };
                if seen.contains(&p.key) {
                    continue;
                }
                seen.push(p.key.clone());
                let Some(a) = self
                    .groups()
                    .flat_map(|g| g.activations.iter())
                    .find(|a| std::sync::Arc::ptr_eq(&a.program, p))
                else {
                    continue;
                };
                let _ = writeln!(out, "Static storage of {}:", p.key);
                for bm in &p.object.modules {
                    for (vi, v) in bm.module.ir.vars.iter().enumerate() {
                        let Some(slot) = bm
                            .module
                            .code
                            .statics
                            .module_var(u32::try_from(vi).unwrap_or(u32::MAX))
                        else {
                            continue;
                        };
                        let at = usize::try_from(bm.frame_offset + slot.offset).unwrap_or(0);
                        let len = usize::try_from(slot.size).unwrap_or(0).min(64);
                        let bytes = a.statics.get(at..at + len).unwrap_or(&[]);
                        let _ = writeln!(
                            out,
                            "  {:<10} {:<16} {}{}",
                            v.name,
                            format!("{}", v.ty),
                            hex(bytes),
                            if slot.size > 64 { " ..." } else { "" }
                        );
                    }
                }
            }
        }
        out
    }
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02X}");
    }
    s
}
