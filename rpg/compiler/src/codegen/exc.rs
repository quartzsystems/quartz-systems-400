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
    /// `*DETC`, `*DETL`, `*TOTC`, `*TOTL`, `*OFL`, `*INIT`: resumed at
    /// the cycle's top for now.
    pub const OTHER: i32 = 4;
    /// An exception the subroutine did not handle; the caller sends it
    /// again.
    pub const FAILED: i32 = 5;
}

/// The code of an `ENDSR` return point.
#[must_use]
pub fn return_point(word: &str) -> i32 {
    match word.trim().to_ascii_uppercase().as_str() {
        "" => rp::NONE,
        "*GETIN" => rp::GETIN,
        "*CANCL" => rp::CANCL,
        _ => rp::OTHER,
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
