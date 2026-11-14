// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Manage` service (version 7, draft; docs/QAP.md "Service 8
//! `Manage`"): each request decoded, the exit point
//! [`EXIT_POINT`] asked, then the host's answer, paged where the
//! operation lists. The host does what the command does, with its
//! authority checks and audit entries.

use ipc::Wire;
use qap_proto::ops::manage::{
    EndHow, EndSubsystem, GetSystemValues, ListHardwareResources, ListPtfs, ListSubsystems,
    PowerDown, Ptfs, SetSystemValue, SubsystemRef, Subsystems,
};
use qap_proto::{Envelope, Op, Status};

use crate::conn::Answer;
use crate::host::{Refusal, ThreadHost};

/// The exit point every `Manage` request passes (`WRKREGINF`).
pub const EXIT_POINT: &str = "QS400_QAP_MGMT";
/// Its format.
pub const EXIT_FORMAT: &str = "QAPX0100";
/// The message of a host without management.
pub const NO_MANAGE: &str = "TCP1A5D";
/// The message when an exit program refuses a request.
pub const EXIT_REFUSED: &str = "TCP1A5E";

/// The server name `Manage` requests carry.
pub const SERVER_MANAGE: &str = "*QAPMGMT";

/// A request as the exit program sees it (`QAPX0100`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExitRequest {
    /// The signed-on profile.
    pub user: String,
    /// The server field: [`SERVER_MANAGE`], or
    /// `crate::netserver::SERVER` for the `NetServer` service,
    /// `crate::nfs::SERVER` for `Nfs`, `crate::dirsrv::SERVER` for
    /// `Dirsrv`; empty is [`SERVER_MANAGE`].
    pub server: String,
    /// The operation's code within its service (`Manage` 1 to 10,
    /// `NetServer` and `Nfs` 1 to 13, `Dirsrv` 1 to 24).
    pub op: u16,
    /// The operation's name (`EndSubsystem`).
    pub name: String,
    /// What it acts on: `LIB/SBSD`, a system value, a product,
    /// `*ALL`; empty when nothing.
    pub target: String,
    /// The rest of the request as text (`*CNTRLD 600`, a new value).
    pub detail: String,
}

/// Blank-padded CP037, cut to `n` bytes.
fn char_field(s: &str, n: usize) -> Vec<u8> {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_else(|_| vec![0x6F; s.chars().count()]);
    b.resize(n, 0x40);
    b
}

impl ExitRequest {
    /// The `QAPX0100` layout: user CHAR(10), server CHAR(10)
    /// (`*QAPMGMT`, `*QAPNETSVR`), format CHAR(8), operation BINARY(4), operation name
    /// CHAR(20), target CHAR(64), detail length BINARY(4), detail
    /// CHAR(*). Big-endian, CP037.
    #[must_use]
    pub fn format(&self) -> Vec<u8> {
        let mut b = char_field(&self.user, 10);
        let server = if self.server.is_empty() {
            SERVER_MANAGE
        } else {
            &self.server
        };
        b.extend(char_field(server, 10));
        b.extend(char_field(EXIT_FORMAT, 8));
        b.extend(u32::from(self.op).to_be_bytes());
        b.extend(char_field(&self.name, 20));
        b.extend(char_field(&self.target, 64));
        let detail = ccsid::to_ebcdic(&self.detail).unwrap_or_default();
        b.extend(
            u32::try_from(detail.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        b.extend(detail);
        b
    }
}

/// `LIB/NAME` of a description.
fn qualified(o: &qap_proto::types::ObjectName) -> String {
    format!("{}/{}", o.library.trim(), o.name.trim())
}

fn how(h: EndHow) -> &'static str {
    match h {
        EndHow::Controlled => "*CNTRLD",
        EndHow::Immediate => "*IMMED",
    }
}

fn delay(d: Option<u32>) -> String {
    d.map_or_else(|| "*NOLIMIT".to_string(), |d| d.to_string())
}

/// The exit request for `op` with body `req`; `None` when the body
/// does not decode.
fn exit_request(user: &str, op: Op, req: &Envelope) -> Option<ExitRequest> {
    let (target, detail) = match op {
        Op::ListSubsystems => (req.body_as::<ListSubsystems>().ok()?.library, String::new()),
        Op::StartSubsystem => (
            qualified(&req.body_as::<SubsystemRef>().ok()?.sbsd),
            String::new(),
        ),
        Op::EndSubsystem => {
            let b = req.body_as::<EndSubsystem>().ok()?;
            let t = b
                .sbsd
                .as_ref()
                .map_or_else(|| "*ALL".to_string(), qualified);
            (t, format!("{} {}", how(b.how), delay(b.delay)))
        }
        Op::GetSystemValues => {
            let b = req.body_as::<GetSystemValues>().ok()?;
            let t = if b.names.is_empty() {
                "*ALL".to_string()
            } else {
                b.names.join(" ")
            };
            (t, String::new())
        }
        Op::SetSystemValue => {
            let b = req.body_as::<SetSystemValue>().ok()?;
            (b.name, b.value)
        }
        Op::ListPtfs => (req.body_as::<ListPtfs>().ok()?.product, String::new()),
        Op::ListHardwareResources => (
            req.body_as::<ListHardwareResources>()
                .ok()?
                .kind
                .special()
                .to_string(),
            String::new(),
        ),
        Op::PowerDown => {
            let b = req.body_as::<PowerDown>().ok()?;
            let restart = if b.restart { "*YES" } else { "*NO" };
            (
                String::new(),
                format!("{} {} RESTART({restart})", how(b.how), delay(b.delay)),
            )
        }
        Op::ListDiskUnits | Op::IplStatus => (String::new(), String::new()),
        _ => return None,
    };
    let (_, code) = op.code();
    Some(ExitRequest {
        user: user.to_string(),
        server: SERVER_MANAGE.to_string(),
        op: code,
        name: format!("{op:?}"),
        target,
        detail,
    })
}

/// A part of a list: `start` 0-based, `max` 0 for all.
pub(crate) fn page<T>(all: Vec<T>, start: u32, max: u32) -> (u32, Vec<T>) {
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let it = all.into_iter().skip(start);
    let v = if max == 0 {
        it.collect()
    } else {
        it.take(usize::try_from(max).unwrap_or(usize::MAX))
            .collect()
    };
    (total, v)
}

fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

fn ok(r: Result<impl Wire, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// Answer one `Manage` operation as `user`, on a secondary thread.
pub(crate) fn serve(host: &mut dyn ThreadHost, user: &str, op: Op, req: &Envelope) -> Answer {
    let Some(exit) = exit_request(user, op, req) else {
        return Answer::status(Status::InvalidRequest);
    };
    if let Err(r) = host.exit_allows(EXIT_POINT, EXIT_FORMAT, &exit) {
        return Answer::refused(r);
    }
    match op {
        Op::ListSubsystems => body(req, |b: ListSubsystems| {
            ok(host.subsystems(&b.library).map(|all| {
                let (total, subsystems) = page(all, b.start, b.max);
                Subsystems { total, subsystems }
            }))
        }),
        Op::StartSubsystem => body(req, |b: SubsystemRef| done(host.start_subsystem(&b.sbsd))),
        Op::EndSubsystem => body(req, |b: EndSubsystem| done(host.end_subsystem(&b))),
        Op::GetSystemValues => body(req, |b: GetSystemValues| ok(host.system_values(&b.names))),
        Op::SetSystemValue => body(req, |b: SetSystemValue| ok(host.set_system_value(&b))),
        Op::ListPtfs => body(req, |b: ListPtfs| {
            ok(host.ptfs(&b.product).map(|all| {
                let (total, ptfs) = page(all, b.start, b.max);
                Ptfs { total, ptfs }
            }))
        }),
        Op::ListDiskUnits => ok(host.disk_units()),
        Op::ListHardwareResources => body(req, |b: ListHardwareResources| {
            ok(host.hardware_resources(b.kind))
        }),
        Op::IplStatus => ok(host.ipl_status()),
        Op::PowerDown => body(req, |b: PowerDown| done(host.power_down(&b))),
        _ => Answer::status(Status::InvalidRequest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exit_layout() {
        let r = ExitRequest {
            user: "BOB".into(),
            server: String::new(),
            op: 3,
            name: "EndSubsystem".into(),
            target: "QSYS/QBATCH".into(),
            detail: "*CNTRLD 60".into(),
        };
        let b = r.format();
        assert_eq!(b.len(), 10 + 10 + 8 + 4 + 20 + 64 + 4 + 10);
        assert_eq!(ccsid::from_ebcdic(&b[..10]), "BOB       ");
        assert_eq!(ccsid::from_ebcdic(&b[10..20]), "*QAPMGMT  ");
        assert_eq!(ccsid::from_ebcdic(&b[20..28]), "QAPX0100");
        assert_eq!(b[28..32], [0, 0, 0, 3]);
        assert_eq!(b[116..120], [0, 0, 0, 10]);
        assert_eq!(ccsid::from_ebcdic(&b[120..]), "*CNTRLD 60");
    }

    #[test]
    fn pages() {
        assert_eq!(page(vec![1, 2, 3, 4], 1, 2), (4, vec![2, 3]));
        assert_eq!(page(vec![1, 2, 3], 0, 0), (3, vec![1, 2, 3]));
        assert_eq!(page(vec![1, 2], 5, 1), (2, Vec::<i32>::new()));
    }
}
