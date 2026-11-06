// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The work management exit points' formats (`EWCPRSEP`, `EWCPWRD`,
//! `EJOBNTFY`): `PRER0100` before `ENDSYS` or `ENDSBS *ALL` reach the
//! restricted state, `PWRD0100` and `PWRD0200` before `PWRDWNSYS`,
//! `ATTN0100` before the attention program, and the job notification
//! entries `NTFY0100` puts on a data queue.

use super::{ParmList, Usage, parm};
use crate::spec::{Layout, field};
use crate::text;

/// What `PRER0100` and `PWRD0200` ask, by action.
pub mod action {
    /// Can the state be reached (the program sets status and wait).
    pub const CHECK: char = '1';
    /// Prepare for it.
    pub const EXECUTE: char = '2';
    /// Undo what the check prepared.
    pub const CANCEL: char = '3';
}

/// The delay time's special values.
pub mod delay {
    /// `OPTION(*IMMED)`.
    pub const IMMED: i32 = -1;
    /// `OPTION(*CNTRLD) DELAY(*NOLIMIT)`.
    pub const NOLIMIT: i32 = -2;
}

/// `PRSE0100` and `PDPF0100`: the check's answer.
pub static CHECK_INFO: Layout = Layout {
    name: "QwcCheckInfo",
    size: 12,
    fields: &[
        field("LEN", 0, 4),
        field("STATUS", 4, 4),
        field("WAIT", 8, 4),
    ],
};

/// `PRSE0200`: execute or cancel.
pub static PRSE0200: Layout = Layout {
    name: "PRSE0200",
    size: 4,
    fields: &[field("LEN", 0, 4)],
};

/// `PDPF0200`: execute or cancel, with the time the program has.
pub static PDPF0200: Layout = Layout {
    name: "PDPF0200",
    size: 8,
    fields: &[field("LEN", 0, 4), field("EXECWAIT", 4, 4)],
};

/// The job start and end entry (`Qwt_Job_Start_End_Notify`).
pub static NTFY_JOB: Layout = Layout {
    name: "QwtJobNotify",
    size: 144,
    fields: &[
        field("MSGID", 0, 10),
        field("MSGFMT", 10, 2),
        field("INTJOBID", 12, 16),
        field("JOB", 28, 26),
        field("JOBQ", 54, 20),
        field("ENTERED", 74, 8),
        field("STARTED", 82, 8),
        field("ENDED", 90, 8),
        field("TYPE", 98, 1),
        field("SUBTYPE", 99, 1),
        field("ENDSEV", 100, 4),
        field("CPUMS", 104, 8),
        field("JOBQASP", 112, 10),
        field("RSV2", 122, 22),
    ],
};

/// The job queue entry (`Qwt_Job_Queue_Notify`).
pub static NTFY_JOBQ: Layout = Layout {
    name: "QwtJobqNotify",
    size: 144,
    fields: &[
        field("MSGID", 0, 10),
        field("MSGFMT", 10, 2),
        field("INTJOBID", 12, 16),
        field("JOB", 28, 26),
        field("JOBQ", 54, 20),
        field("ENTERED", 74, 8),
        field("RSV", 82, 16),
        field("TYPE", 98, 1),
        field("SUBTYPE", 99, 1),
        field("RSV2", 100, 12),
        field("JOBQASP", 112, 10),
        field("RSV3", 122, 22),
    ],
};

/// The job notification registration's program data.
pub static NTFY_DATA: Layout = Layout {
    name: "QwtNotifyData",
    size: 24,
    fields: &[
        field("NTFYTYPE", 0, 4),
        field("SBSD", 4, 10),
        field("SBSDLIB", 14, 10),
    ],
};

/// `PRER0100` and `PWRD0200`.
pub static PREPARE: ParmList = ParmList {
    name: "PREPARE",
    parms: &[
        parm(1, Usage::InOut, "CHAR(*)", "Return information"),
        parm(2, Usage::Input, "CHAR(8)", "Format of return information"),
        parm(3, Usage::Input, "CHAR(1)", "Action"),
        parm(4, Usage::Input, "BINARY(4)", "Delay time"),
    ],
};

/// The parameter lists of this module (`PWRD0100` and `ATTN0100` have
/// none).
pub static LISTS: &[&ParmList] = &[&PREPARE];

/// A `PRER0100` or `PWRD0200` call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepare {
    /// `PRER0100` or `PWRD0200`.
    pub format: String,
    /// [`action`].
    pub action: char,
    /// [`delay`], or seconds.
    pub delay: i32,
    /// For execute: the seconds the program has (`PDPF0200`).
    pub exec_wait: i32,
}

/// The return information's format name for `p`.
#[must_use]
pub fn return_format(p: &Prepare) -> &'static str {
    match (p.format.as_str(), p.action) {
        ("PWRD0200", action::CHECK) => "PDPF0100",
        ("PWRD0200", _) => "PDPF0200",
        (_, action::CHECK) => "PRSE0100",
        _ => "PRSE0200",
    }
}

/// The parameters of a `PRER0100` or `PWRD0200` call.
#[must_use]
pub fn prepare_parms(p: &Prepare) -> Vec<Vec<u8>> {
    let info = match return_format(p) {
        "PRSE0100" | "PDPF0100" => {
            let mut r = CHECK_INFO.zeroed();
            // The status starts at 1: a program that sets nothing allows.
            r.bin4("LEN", 12).bin4("STATUS", 1).bin4("WAIT", 0);
            r.into_bytes()
        }
        "PDPF0200" => {
            let mut r = PDPF0200.zeroed();
            r.bin4("LEN", 8).bin4("EXECWAIT", p.exec_wait);
            r.into_bytes()
        }
        _ => {
            let mut r = PRSE0200.zeroed();
            r.bin4("LEN", 4);
            r.into_bytes()
        }
    };
    let mut d = [0u8; 4];
    d.copy_from_slice(&p.delay.to_be_bytes());
    vec![
        info,
        text::fixed(return_format(p), 8),
        text::fixed(&p.action.to_string(), 1),
        d.to_vec(),
    ]
}

/// A check's answer: whether the state can be reached and the seconds
/// the program needs. A status other than 0 or 1 counts as 1, as IBM
/// treats it.
#[must_use]
pub fn check_answer(parms: &[Vec<u8>]) -> (bool, i32) {
    let b = parms.first().map_or(&[][..], Vec::as_slice);
    let status = text::bin4(b, 4);
    let wait = text::bin4(b, 8).clamp(0, 3600);
    (status != 0, wait)
}
