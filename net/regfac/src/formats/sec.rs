// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security exit points' formats (`ECHGPRF1`, `ECRTPRF1`,
//! `EDLTPRF1`, `EDLTPRF2`, `ERSTPRF1`, `EVLDPWD1`, `ECHKPWD1`): the
//! profile notifications `CHGP0100`, `CHGP0200`, `CRTP0100`,
//! `DLTP0100`, `DLTP0200` and `RSTP0100`, one structure naming the
//! profile, and the password checks `VLDP0100` and `CHKP0100`, the
//! passwords after a fixed part and a one-character answer (`0`
//! accepts).

use super::{ParmList, Usage, parm};
use crate::spec::{Layout, field};

/// The profile notification (`Qsy_Change_Profile_Info` and its kin).
pub static PROFILE_INFO: Layout = Layout {
    name: "QsyProfileInfo",
    size: 38,
    fields: &[
        field("EXITPNT", 0, 20),
        field("EXITFMT", 20, 8),
        field("USRPRF", 28, 10),
    ],
};

/// The password check's fixed part (`Qsy_Vld_Password_Info`,
/// `Qsy_Chk_Password_Info`); the old password, then the new, follow.
pub static PASSWORD_INFO: Layout = Layout {
    name: "QsyPasswordInfo",
    size: 68,
    fields: &[
        field("EXITPNT", 0, 20),
        field("EXITFMT", 20, 8),
        field("PWDLVL", 28, 4),
        field("USRPRF", 32, 10),
        field("RSV", 42, 2),
        field("OFFOLD", 44, 4),
        field("LENOLD", 48, 4),
        field("CCSIDOLD", 52, 4),
        field("OFFNEW", 56, 4),
        field("LENNEW", 60, 4),
        field("CCSIDNEW", 64, 4),
    ],
};

/// The profile notifications' one parameter.
pub static PROFILE: ParmList = ParmList {
    name: "PROFILE",
    parms: &[parm(1, Usage::Input, "CHAR(*)", "Profile exit information")],
};

/// `VLDP0100` and `CHKP0100`.
pub static PASSWORD: ParmList = ParmList {
    name: "PASSWORD",
    parms: &[
        parm(1, Usage::Input, "CHAR(*)", "Password exit information"),
        parm(2, Usage::Output, "CHAR(1)", "Return indicator"),
    ],
};

/// The parameter lists of this module.
pub static LISTS: &[&ParmList] = &[&PROFILE, &PASSWORD];

/// The profile notification formats.
pub const PROFILE_FORMATS: &[&str] = &[
    "CHGP0100", "CHGP0200", "CRTP0100", "DLTP0100", "DLTP0200", "RSTP0100",
];

/// The passwords' CCSID: UCS-2, as IBM passes them.
pub const PWD_CCSID: i32 = 13488;

/// What stands for an old password the interface does not have.
pub const NO_PASSWORD: &str = "*NOPWD";

/// A password check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Password {
    /// `VLDP0100` or `CHKP0100`.
    pub format: String,
    /// The exit point.
    pub point: String,
    /// `QPWDLVL`.
    pub level: i32,
    /// The profile.
    pub user: String,
    /// The current password, `None` when the interface has none.
    pub old: Option<String>,
    /// The new password.
    pub new: String,
}

fn ucs2(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

fn n32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// A profile notification's structure.
#[must_use]
pub fn profile_info(point: &str, format: &str, user: &str) -> Vec<u8> {
    let mut r = PROFILE_INFO.blank();
    r.text("EXITPNT", point)
        .text("EXITFMT", format)
        .text("USRPRF", user);
    r.into_bytes()
}

/// A password check's parameters: the information and the answer,
/// blank until a program says `0`.
#[must_use]
pub fn password_parms(p: &Password) -> Vec<Vec<u8>> {
    let old = ucs2(p.old.as_deref().unwrap_or(NO_PASSWORD));
    let new = ucs2(&p.new);
    let mut r = PASSWORD_INFO.blank();
    let at_old = PASSWORD_INFO.size;
    let at_new = at_old + old.len();
    r.text("EXITPNT", &p.point)
        .text("EXITFMT", &p.format)
        .bin4("PWDLVL", p.level)
        .text("USRPRF", &p.user)
        .bytes("RSV", &[])
        .bin4("OFFOLD", n32(at_old))
        .bin4("LENOLD", n32(old.len()))
        .bin4("CCSIDOLD", PWD_CCSID)
        .bin4("OFFNEW", n32(at_new))
        .bin4("LENNEW", n32(new.len()))
        .bin4("CCSIDNEW", PWD_CCSID);
    let mut b = r.into_bytes();
    b.extend(old);
    b.extend(new);
    vec![b, vec![0x40]]
}
