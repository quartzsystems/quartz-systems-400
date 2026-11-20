// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSI server's commands (docs/QSID.md section 12), QS/400's:
//! `STRQSIPAIR` makes a pairing password and shows it once on a panel
//! (`panels/qsi.rs`; a job without a display is refused, so the password
//! is never in a job log), `RMVQSICSL` removes a console's pin,
//! `WRKQSICSL` shows the consoles (a panel at a display, text
//! otherwise). The storage service checks the authority (`*ALLOBJ` and
//! `*SECADM`) and writes the `QI` audit entries.

use std::fmt::Write as _;

use cmd::Resolved;
use stg::Client;
use stg::qsi::{QsiConsole, QsiOp, QsiReply};

use super::{Fail, parms_of};
use crate::console::host::Host;

/// Ask the service.
pub fn ask(host: &Host, op: QsiOp) -> QsiReply {
    host.link.borrow_mut().with(|l| l.qsi(op))
}

/// A refusal as the command's escape.
pub fn fail(r: QsiReply) -> Fail {
    match r {
        QsiReply::Refused { notes } => notes.into_iter().next().map_or_else(
            || Fail::check("", "a refusal without a message"),
            |(id, t)| Fail::new(&id, t),
        ),
        other => Fail::check("", format!("unexpected answer {other:?}")),
    }
}

/// A moment as the job shows it: `MM/DD/YY HH:MM:SS` at the system's
/// offset, with seconds or not.
pub fn when(host: &Host, secs: u64, seconds: bool) -> String {
    if secs == 0 {
        return String::new();
    }
    let offset = match ask(host, QsiOp::Time) {
        QsiReply::Time { time } => time.offset_s,
        _ => 0,
    };
    let (d, t) = crate::httpenv::stamp::stamp(secs, offset, "*MDY", "/", ":");
    if seconds {
        format!("{d}  {t}")
    } else {
        format!("{d} {}", t.get(..5).unwrap_or(&t))
    }
}

/// The system's fingerprint, or why there is none.
pub fn fingerprint(host: &Host) -> Result<String, Fail> {
    match ask(host, QsiOp::Fingerprint) {
        QsiReply::Fingerprint { hex, .. } => Ok(hex),
        QsiReply::NoIdentity => Err(Fail::new(
            servers::msg::QSI_NO_IDENTITY,
            "The system has no QSI identity yet.",
        )),
        r => Err(fail(r)),
    }
}

/// The consoles paired.
pub fn consoles(host: &Host) -> Result<Vec<QsiConsole>, Fail> {
    match ask(host, QsiOp::Consoles) {
        QsiReply::Consoles { list } => Ok(list),
        r => Err(fail(r)),
    }
}

/// `RMVQSICSL CONSOLE()`.
pub fn rmvqsicsl(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let console = parms.text("CONSOLE").trim().trim_matches('\'').to_string();
    match ask(
        host,
        QsiOp::Remove {
            console,
            by: String::new(),
        },
    ) {
        QsiReply::Removed { console } => Ok(format!(
            "{} Management console {} removed.\n",
            servers::msg::QSI_REMOVED,
            console.name
        )),
        r => Err(fail(r)),
    }
}

/// `WRKQSICSL` without a display: the fingerprint and the list as text.
pub fn wrkqsicsl_text(host: &Host) -> Result<String, Fail> {
    let list = consoles(host)?;
    let fp = fingerprint(host).unwrap_or_else(|_| "(none yet)".into());
    let mut out = format!("System's QSI fingerprint: {fp}\n");
    if list.is_empty() {
        out.push_str("No management consoles are paired.\n");
    }
    for (i, c) in list.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}. {}  paired {}  last session {}  from {}  fingerprint {}",
            i + 1,
            c.name,
            when(host, c.paired, false),
            if c.last_contact == 0 {
                "never".to_string()
            } else {
                when(host, c.last_contact, false)
            },
            if c.address.is_empty() {
                "-"
            } else {
                &c.address
            },
            c.fingerprint
        );
    }
    Ok(out)
}

/// The commands of this module; `None` for another command.
pub fn run(host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "RMVQSICSL" => rmvqsicsl(host, r),
        "WRKQSICSL" => wrkqsicsl_text(host),
        // The password is shown on a display only.
        "STRQSIPAIR" => Err(Fail::no_display("STRQSIPAIR")),
        _ => return None,
    })
}
