// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! System reference codes. One table, no scattered strings.
//!
//! Code families: `C1` LIC init, `C6` OS init, `A6` attention (non-fatal),
//! `B1` fatal, `D1` power down.

/// Where in the IPL a code belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// LIC initialization.
    Lic,
    /// Operating system initialization.
    Os,
    /// Attention, continues.
    Attention,
    /// Fatal, parks.
    Fatal,
    /// Power down.
    PowerDown,
}

/// One reference code.
#[derive(Debug, PartialEq, Eq)]
pub struct Src {
    /// Eight-character code.
    pub code: &'static str,
    /// Short Quartz-written text.
    pub text: &'static str,
    /// Phase.
    pub phase: Phase,
}

const fn src(code: &'static str, text: &'static str, phase: Phase) -> Src {
    Src { code, text, phase }
}

/// IPL started.
pub const IPL_STARTED: Src = src("C1001000", "IPL started", Phase::Lic);
/// LIC partition located.
pub const LIC_LOCATED: Src = src("C1001010", "LIC partition located", Phase::Lic);
/// LIC record read (or initialized).
pub const LIC_READ: Src = src("C1001020", "LIC record read", Phase::Lic);
/// Hardware resources enumerated.
pub const HW_ENUMERATED: Src = src("C1001030", "Hardware resources enumerated", Phase::Lic);
/// Clock set from RTC.
pub const CLOCK_SET: Src = src("C1001040", "Clock set from RTC", Phase::Lic);
/// Console attached. Emitted once the console crate attaches a display.
#[allow(dead_code)]
pub const CONSOLE_ATTACHED: Src = src("C1001050", "Console attached", Phase::Lic);
/// Network links enumerated.
pub const NET_ENUMERATED: Src = src("C1001060", "Network links enumerated", Phase::Lic);
/// Previous IPL was abnormal. Informational, continues.
pub const PREV_ABNORMAL: Src = src("C1002000", "Previous IPL was abnormal", Phase::Lic);
/// OS phase started.
pub const OS_STARTED: Src = src("C6001000", "OS phase started", Phase::Os);
/// System values loaded.
pub const SYSVALS_LOADED: Src = src("C6001010", "System values loaded", Phase::Os);
/// Startup program skipped. MIGRATE(phase5): run QSTRUPPGM.
pub const STRUP_SKIPPED: Src = src(
    "C6001020",
    "Startup program skipped, no object store",
    Phase::Os,
);
/// IPL complete.
pub const IPL_COMPLETE: Src = src("C6001090", "IPL complete", Phase::Os);
/// Console not found; continuing on serial.
pub const CONSOLE_MISSING: Src = src(
    "A6005008",
    "Console not found, continuing on serial",
    Phase::Attention,
);
/// slic panicked.
pub const PANIC: Src = src("B1001000", "slic panic", Phase::Fatal);
/// LIC partition not found.
pub const LIC_NOT_FOUND: Src = src("B1001010", "LIC partition not found", Phase::Fatal);
/// LIC record unreadable in both copies.
pub const LIC_UNREADABLE: Src = src(
    "B1001020",
    "LIC record unreadable, both copies",
    Phase::Fatal,
);
/// Power down started.
pub const PWRDWN_STARTED: Src = src("D1001000", "Power down started", Phase::PowerDown);
/// Power down complete.
pub const PWRDWN_COMPLETE: Src = src("D1001090", "Power down complete", Phase::PowerDown);

/// Every code, in table order.
pub const TABLE: &[Src] = &[
    IPL_STARTED,
    LIC_LOCATED,
    LIC_READ,
    HW_ENUMERATED,
    CLOCK_SET,
    CONSOLE_ATTACHED,
    NET_ENUMERATED,
    PREV_ABNORMAL,
    OS_STARTED,
    SYSVALS_LOADED,
    STRUP_SKIPPED,
    IPL_COMPLETE,
    CONSOLE_MISSING,
    PANIC,
    LIC_NOT_FOUND,
    LIC_UNREADABLE,
    PWRDWN_STARTED,
    PWRDWN_COMPLETE,
];

/// Find a code in the table.
pub fn lookup(code: &str) -> Option<&'static Src> {
    TABLE.iter().find(|s| s.code == code)
}

impl Src {
    /// The serial line: `SRC <code>  <text>`.
    pub fn line(&self) -> String {
        format!("SRC {}  {}\n", self.code, self.text)
    }

    /// The code as fixed 8 ASCII bytes for the LIC history record.
    pub fn bytes(&self) -> [u8; 8] {
        let mut b = *b"        ";
        for (dst, src) in b.iter_mut().zip(self.code.bytes()) {
            *dst = src;
        }
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_well_formed() {
        for (i, s) in TABLE.iter().enumerate() {
            assert_eq!(s.code.len(), 8, "{}", s.code);
            assert!(
                s.code.bytes().all(|b| b.is_ascii_alphanumeric()),
                "{}",
                s.code
            );
            assert!(!s.text.is_empty());
            assert!(
                TABLE[..i].iter().all(|o| o.code != s.code),
                "duplicate {}",
                s.code
            );
        }
        assert_eq!(lookup("C6001090"), Some(&IPL_COMPLETE));
    }
}
