// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a profile limits: `LMTCPB` at sign-on and on the command line,
//! `PTYLMT`, `MAXSTG`, `USREXPDATE`.

use crate::profile::payload::{LmtCpb, UsrPrf};
use crate::{Result, SecError};

/// The commands a `LMTCPB(*YES)` user may run until `*CMD` objects carry
/// `ALLOW(*LIMITED)` (`MIGRATE(phase7)`).
pub const LIMITED_COMMANDS: &[&str] = &["SIGNOFF", "DSPMSG", "DSPJOB", "DSPJOBLOG", "WRKMSG"];

/// Which sign-on fields the user may fill in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SignOnFields {
    /// Program/procedure.
    pub program: bool,
    /// Menu.
    pub menu: bool,
    /// Current library.
    pub curlib: bool,
    /// Attention program (`CHGPRF`).
    pub atnpgm: bool,
}

/// What `LMTCPB` lets the user change at sign-on.
#[must_use]
pub fn sign_on_fields(p: &UsrPrf) -> SignOnFields {
    match p.lmtcpb {
        LmtCpb::No => SignOnFields {
            program: true,
            menu: true,
            curlib: true,
            atnpgm: true,
        },
        LmtCpb::Partial => SignOnFields {
            program: false,
            menu: true,
            curlib: false,
            atnpgm: false,
        },
        LmtCpb::Yes => SignOnFields {
            program: false,
            menu: false,
            curlib: false,
            atnpgm: false,
        },
    }
}

/// Whether the user may run command `cmd` from a command line.
#[must_use]
pub fn command_allowed(p: &UsrPrf, cmd: &str) -> bool {
    p.lmtcpb != LmtCpb::Yes || LIMITED_COMMANDS.iter().any(|c| c.eq_ignore_ascii_case(cmd))
}

/// `CPF2213`-style refusal for a limited user's command.
pub fn require_command(p: &UsrPrf, cmd: &str) -> Result<()> {
    if command_allowed(p, cmd) {
        Ok(())
    } else {
        Err(SecError::msg(
            "CPD0032",
            format!("{} is limited and may not run {cmd}.", p.name),
        ))
    }
}

/// A scheduling priority (`JOBPTY`, `OUTPTY`, `RUNPTY`) no higher than
/// `PTYLMT` allows: a smaller number is a higher priority.
#[must_use]
pub fn clamp_priority(p: &UsrPrf, value: u8) -> u8 {
    value.max(p.ptylmt)
}

/// Whether `add_kb` more storage fits `MAXSTG`; `CPF1151` when not.
pub fn storage(p: &UsrPrf, add_kb: i64) -> Result<()> {
    if p.maxstg < 0 || add_kb <= 0 {
        return Ok(());
    }
    let used = i64::try_from(p.stg_used).unwrap_or(i64::MAX);
    if used.saturating_add(add_kb) > p.maxstg {
        return Err(SecError::msg(
            "CPF1151",
            format!(
                "{} would use more than its {} KB storage limit.",
                p.name, p.maxstg
            ),
        ));
    }
    Ok(())
}

/// Whether `USREXPDATE` has passed.
#[must_use]
pub fn user_expired(p: &UsrPrf, now: u64) -> bool {
    p.usrexpdate != 0 && now >= p.usrexpdate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits() {
        let mut p = UsrPrf::new("BOB", 40);
        assert!(command_allowed(&p, "CRTLIB"));
        p.lmtcpb = LmtCpb::Yes;
        assert!(!command_allowed(&p, "CRTLIB"));
        assert!(command_allowed(&p, "signoff"));
        assert!(require_command(&p, "CALL").is_err());
        assert!(!sign_on_fields(&p).menu);
        p.lmtcpb = LmtCpb::Partial;
        assert!(sign_on_fields(&p).menu && !sign_on_fields(&p).program);
        p.ptylmt = 3;
        assert_eq!(clamp_priority(&p, 1), 3);
        assert_eq!(clamp_priority(&p, 5), 5);
        p.maxstg = 100;
        p.stg_used = 90;
        assert!(storage(&p, 8).is_ok());
        assert_eq!(storage(&p, 16).unwrap_err().msgid(), Some("CPF1151"));
        p.usrexpdate = 1000;
        assert!(!user_expired(&p, 999) && user_expired(&p, 1000));
    }
}
