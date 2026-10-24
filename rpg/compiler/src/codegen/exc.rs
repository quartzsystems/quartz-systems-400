// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Exceptions as RPG sees them: the program status data structure, the
//! last status (`%STATUS`) and error indicator (`%ERROR`), the `(E)`
//! extender, and handing an exception on when `*PSSR` leaves it
//! unhandled. Handlers update the PSDS from the exception the runtime
//! just handled (`_QRNX_PSDSERR`); an operation with `(E)` runs inside
//! a monitor region of its own.

use crate::codegen::Gen;
use crate::codegen::emit::quote;
use crate::names::mangle;
use crate::syms::Kind;

/// Return-point codes a main-procedure subroutine returns.
pub mod rp {
    /// `ENDSR` with no return point (or `LEAVESR`).
    pub const NONE: i32 = 0;
    /// `RETURN` inside the subroutine.
    pub const RETURN: i32 = 1;
    /// `*GETIN`.
    pub const GETIN: i32 = 2;
    /// `*CANCL`.
    pub const CANCL: i32 = 3;
    /// `*INIT` and anything else: the cycle's top.
    pub const OTHER: i32 = 4;
    /// An exception the subroutine did not handle; the caller sends it
    /// again.
    pub const FAILED: i32 = 5;
    /// `*DETC`: the detail calculations.
    pub const DETC: i32 = 6;
    /// `*DETL`: the detail lines (the cycle's top).
    pub const DETL: i32 = 7;
    /// `*TOTC`: the total calculations.
    pub const TOTC: i32 = 8;
    /// `*TOTL`: the total lines.
    pub const TOTL: i32 = 9;
    /// `*OFL`: the overflow lines.
    pub const OFL: i32 = 10;
    /// The points that resume the cycle.
    pub const RESUME: [i32; 7] = [GETIN, OTHER, DETC, DETL, TOTC, TOTL, OFL];
}

/// The code of an `ENDSR` return point.
#[must_use]
pub fn return_point(word: &str) -> i32 {
    match word.trim().to_ascii_uppercase().as_str() {
        "" => rp::NONE,
        "*GETIN" => rp::GETIN,
        "*CANCL" => rp::CANCL,
        "*DETC" => rp::DETC,
        "*DETL" => rp::DETL,
        "*TOTC" => rp::TOTC,
        "*TOTL" => rp::TOTL,
        "*OFL" => rp::OFL,
        _ => rp::OTHER,
    }
}

/// The main procedure's handlers: with a `*PSSR` or an `INFSR`, the
/// cycle is one monitor region; program errors go to `*PSSR`, file
/// errors to the failing file's `INFSR` (by `$ferr`), and each handler
/// runs its subroutine and resumes at the region's start or hands the
/// exception on.
pub struct Region {
    pssr: bool,
    /// File number (`$ferr`) and subroutine of each `INFSR`.
    infsrs: Vec<(usize, String)>,
    pssr_l: String,
    infsr_l: String,
    fcheck: String,
}

impl Region {
    /// The handlers a main procedure needs, from its subroutines; none
    /// when it has neither `*PSSR` nor an `INFSR`.
    pub fn of(g: &mut Gen<'_>, subrs: &[String]) -> Option<Self> {
        let known = |g: &Gen<'_>, s: &str| {
            subrs.iter().find(|n| n.eq_ignore_ascii_case(s) && !g.bad_subrs.contains(&n.to_ascii_uppercase())).cloned()
        };
        let pssr = subrs.iter().any(|n| n == "*PSSR");
        let infsrs: Vec<(usize, String)> = g
            .files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.scope == 0)
            .filter_map(|(k, f)| known(g, f.infsr.as_ref()?).map(|n| (k + 1, n)))
            .collect();
        if !pssr && infsrs.is_empty() {
            return None;
        }
        Some(Self {
            pssr,
            infsrs,
            pssr_l: g.b.label(),
            infsr_l: g.b.label(),
            fcheck: g.b.label(),
        })
    }

    /// Open the region: placed where the handlers resume.
    pub fn open(&self, g: &mut Gen<'_>) {
        // The function check of an error neither takes goes on to the
        // default handler, past *PSSR.
        let mut sels = vec![format!("CPF9999 -> {}", self.fcheck)];
        if self.pssr {
            sels.push(format!("*PROGRAM -> {}", self.pssr_l));
        }
        if !self.infsrs.is_empty() {
            sels.push(format!("*FILE -> {}", self.infsr_l));
        }
        g.b.ins(format!("monitor PSSR, {}", sels.join(", ")));
    }

    /// Close the region and place the handlers. A resuming return point
    /// goes back to `top` (the region's start), after its code is stored
    /// in `slot` when there is one; `RETURN` in the subroutine goes to
    /// `end` or `ret` by `LR`; a blank point or `*CANCL` hands the
    /// exception on.
    pub fn close(&self, g: &mut Gen<'_>, top: &str, slot: Option<&str>, end: &str, ret: &str) {
        let close = g.b.label();
        g.b.place(&close);
        g.b.ins("endmon PSSR");
        g.b.term("unreachable");
        let on = g.b.label();
        let finish_out = g.b.label();
        let returned = |g: &mut Gen<'_>, subr: &str| {
            let rc = g.b.reg();
            g.b.ins(format!("{rc} = callb {}()", Gen::subr_proc(subr)));
            if let Some(s) = slot {
                g.b.ins(format!("store {s}, {rc}"));
            }
            let mut arms: Vec<String> = rp::RESUME.iter().map(|c| format!("{c}: {top}")).collect();
            arms.push(format!("{}: {finish_out}", rp::RETURN));
            g.b.term(format!("switch {rc}, [{}], default {on}", arms.join(", ")));
        };
        g.b.place(&self.fcheck);
        g.b.term(format!("br {on}"));
        if self.pssr {
            g.b.place(&self.pssr_l);
            g.psds_error();
            returned(g, "*PSSR");
        }
        if !self.infsrs.is_empty() {
            g.b.place(&self.infsr_l);
            g.psds_error();
            let k = g.b.reg();
            g.b.ins(format!("{k} = load $ferr"));
            let labels: Vec<String> = self.infsrs.iter().map(|_| g.b.label()).collect();
            let arms: Vec<String> = self.infsrs.iter().zip(&labels).map(|((n, _), l)| format!("{n}: {l}")).collect();
            g.b.term(format!("switch {k}, [{}], default {on}", arms.join(", ")));
            for ((_, s), l) in self.infsrs.iter().zip(&labels) {
                g.b.place(l);
                returned(g, s);
            }
        }
        g.b.place(&finish_out);
        let lr = g.b.reg();
        g.b.ins(format!("{lr} = load $inlr"));
        g.b.term(format!("brif {lr}, {end}, {ret}"));
        g.b.place(&on);
        g.reraise();
        let again = g.b.label();
        g.b.term(format!("br {again}"));
        g.b.place(&again);
        g.b.term("ret");
    }
}

impl Gen<'_> {
    /// The program status data structure's whole-structure place and
    /// length, or a one-byte dummy.
    pub fn psds(&self) -> (String, u32) {
        for d in &self.t.ds {
            if d.psds
                && let Some(Kind::Ds(_)) = &self.t.syms[d.sym].kind
            {
                return (format!("{}.all_", self.t.syms[d.sym].ir), d.len);
            }
        }
        ("$psds_none".into(), 0)
    }

    /// Fill the PSDS's fixed fields at program start: the procedure
    /// (program) name and the parameter count.
    pub fn psds_init(&mut self, name: &str) {
        let (p, len) = self.psds();
        if len == 0 {
            return;
        }
        self.rt_proc(
            "_QRNX_PSDSINIT",
            "($ds: bytes(1) *VARSIZE, byval $len: i32, const $name: char(10), byval $parms: i32)",
        );
        let r = self.b.reg();
        self.b.ins(format!("{r} = parms"));
        self.b.ins(format!("callb _QRNX_PSDSINIT({p}, {len}, {}, {r})", quote(&format!("{name:<10}"))));
    }

    /// Note the cycle routine running (`*DETC`, `*TOTC`, ...) in the
    /// PSDS.
    pub fn psds_routine(&mut self, routine: &str) {
        let (p, len) = self.psds();
        if len < 36 {
            return;
        }
        let a = self.b.reg();
        self.b.ins(format!("{a} = addr {p}"));
        self.b.ins(format!("mov [{a} +28]:char(8), {}", quote(&format!("{routine:<8}"))));
    }

    /// At a handler's start: the exception just handled into the PSDS
    /// (status, previous status, statement, message identifier) and the
    /// last status.
    pub fn psds_error(&mut self) {
        let (p, len) = self.psds();
        self.rt_proc(
            "_QRNX_PSDSERR",
            "($ds: bytes(1) *VARSIZE, byval $len: i32, $status: zoned(5,0))",
        );
        self.b.ins(format!("callb _QRNX_PSDSERR({p}, {len}, $status)"));
    }

    /// Send the exception the last handler took on again, as unhandled
    /// here: the program ends the way an unmonitored error ends it.
    pub fn reraise(&mut self) {
        self.rt_proc("_QRNX_RERAISE", "()");
        self.b.ins("callb _QRNX_RERAISE()");
    }

    /// Run `body` as an operation with the `(E)` extender: `%ERROR` and
    /// `%STATUS` cleared before it, set by the handler when it fails.
    pub fn with_error_extender(&mut self, body: impl FnOnce(&mut Self)) {
        self.b.ins("store $errind, *OFF");
        self.b.ins("mov $status, *ZEROS");
        self.monitors += 1;
        let name = format!("E{}", self.monitors);
        let handler = self.b.label();
        let done = self.b.label();
        self.b.ins(format!("monitor {name}, *ALL -> {handler}"));
        body(self);
        self.b.ins(format!("endmon {name}"));
        self.b.term(format!("br {done}"));
        self.b.place(&handler);
        self.psds_error();
        self.b.ins("store $errind, *ON");
        self.b.term(format!("br {done}"));
        self.b.place(&done);
    }

    /// The IR name of a main-procedure subroutine's procedure.
    #[must_use]
    pub fn subr_proc(name: &str) -> String {
        format!("_qrns_{}", mangle(&name.replace('*', "_")))
    }
}
