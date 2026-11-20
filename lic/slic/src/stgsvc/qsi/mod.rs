// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSI server's side of the storage service (`docs/QSID.md` section
//! 12, request `qsi`): what the server job `QSIDSRV` keeps here (its
//! identity sealed by the LIC's key slot, the consoles pinned, its event
//! log, all under `/QIBM/UserData/OS400/QSI`), the pairing password in
//! this process's memory alone, and what a console asks of the system
//! (`system`). Who may ask what:
//!
//! - the server job: everything but making, ending and removing;
//! - a security officer (`*ALLOBJ` and `*SECADM`), through the commands:
//!   making and ending a password, removing a console, the consoles and
//!   the fingerprint;
//! - `slic`, for the service tools: the same as a security officer.

mod keep;
mod system;

use stg::qsi::{QsiOp, QsiReply};

use super::served::JobCtx;
use crate::ipl::system::System;

/// The server job's name.
pub const SERVER_JOB: &str = "QSIDSRV";

/// Who is asking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asker {
    /// The server job.
    Server,
    /// The service tools, through `slic`.
    Dst,
    /// A job whose profile is a security officer.
    Officer,
    /// Anyone else.
    Other,
}

fn asker(ctx: &JobCtx) -> Asker {
    if ctx.job.number == 0 && ctx.job.name == "SCPF" {
        return Asker::Dst;
    }
    if ctx.job.name == SERVER_JOB && ctx.job.user == "QSYS" {
        return Asker::Server;
    }
    let me = ctx.session.borrow().principal.clone();
    if me.has(sec::profile::SpecialAuth::ALLOBJ) && me.has(sec::profile::SpecialAuth::SECADM) {
        Asker::Officer
    } else {
        Asker::Other
    }
}

fn not_allowed(what: &str) -> QsiReply {
    QsiReply::refused(
        "CPF222E",
        &format!("Special authority *ALLOBJ and *SECADM are needed to {what}."),
    )
}

fn server_only() -> QsiReply {
    QsiReply::refused(
        "CPF9898",
        "Only the QSI server job QSIDSRV may ask for this.",
    )
}

/// Answer `op` for the job of `ctx`.
pub fn request(system: &mut System, ctx: &JobCtx, op: QsiOp) -> QsiReply {
    let who = asker(ctx);
    let job_user = ctx.job.user.clone();
    let officer = matches!(who, Asker::Officer | Asker::Dst);
    let server = who == Asker::Server;
    match op {
        QsiOp::Fingerprint if officer || server => keep::fingerprint(system),
        QsiOp::Consoles if officer || server => keep::consoles(system),
        QsiOp::Pairing if officer || server => keep::pairing(),
        QsiOp::Time if officer || server => system::time(system),
        QsiOp::StartPairing { by } if officer => keep::start_pairing(system, who, &by, &job_user),
        QsiOp::EndPairing { by } if officer => keep::end_pairing(who, &by, &job_user),
        QsiOp::Remove { console, by } if officer => {
            keep::remove(system, who, &console, &by, &job_user)
        }
        QsiOp::StartPairing { .. } => not_allowed("make a pairing password"),
        QsiOp::EndPairing { .. } => not_allowed("end a pairing password"),
        QsiOp::Remove { .. } => not_allowed("remove a management console"),
        QsiOp::Fingerprint | QsiOp::Consoles | QsiOp::Pairing => {
            not_allowed("work with the management consoles")
        }
        _ if !server => server_only(),
        QsiOp::Identity => keep::identity(system),
        QsiOp::KeepIdentity { cert, key } => keep::keep_identity(system, cert, key),
        QsiOp::Attempt => keep::attempt(),
        QsiOp::Failed {
            name,
            fingerprint,
            address,
            why,
        } => keep::failed(&name, &fingerprint, &address, &why),
        QsiOp::Paired {
            name,
            fingerprint,
            address,
        } => keep::paired(system, &name, &fingerprint, &address),
        QsiOp::Contact {
            fingerprint,
            address,
        } => keep::contact(system, &fingerprint, &address),
        QsiOp::LoadEvents => keep::load_events(system),
        QsiOp::SaveEvents { bytes } => keep::save_events(system, &bytes),
        QsiOp::Identify => system::identify(system),
        QsiOp::Power {
            immediate,
            delay,
            restart,
            user,
        } => system::power(system, ctx, immediate, delay, restart, &user),
        QsiOp::Time => system::time(system),
        QsiOp::FixLevel => system::fix_level(system),
        QsiOp::Service { op, body, user } => system::service(system, op, &body, &user),
        QsiOp::Problems => system::problems(system),
        QsiOp::Dumps => system::dumps(system),
        QsiOp::TakeDump { why, user } => system::take_dump(system, &why, &user),
        QsiOp::DeleteDump { name, user } => system::delete_dump(system, &name, &user),
        QsiOp::DumpSpace => system::dump_space(system),
        QsiOp::DumpRead { name, offset, len } => system::dump_read(system, &name, offset, len),
    }
}
