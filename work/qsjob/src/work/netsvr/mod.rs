// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer's jobs (`docs/NETSERVER.md` section 3): `QZLSSERVER`, the
//! listener `QSERVER` runs from routing entry 800, and `QZLSFILE`, the
//! prestart job that serves one client with `netserver::NetServer`.

#![allow(clippy::doc_markdown)] // NetServer, NetBIOS

pub mod file;
mod printer;
mod report;
pub mod server;
mod signon;
mod svc;

use std::time::Duration;

use netserver::print::OutQueue;
use netserver::{FileShare, NetServerConfig, PrintShareCfg};
use netsvrcfg::{Config, Kind, Permission};
use stg::{Client, Shared};
use stg::netsvr::{NsOp, NsReply};
use wm::obj::msgq::{MsgKind, QMsg};

/// The role of `QZLSSERVER` started as a server job (the registry's).
pub const ROLE: &str = "NETSVR";
/// The listener's program (routing entry 800 of `QSERVER`).
pub const SERVER_PROGRAM: &str = "QZLSSERVER";
/// The prestart job program connections are handed to.
pub const FILE_PROGRAM: &str = "QZLSFILE";

/// The configuration through the service (`NsOp::Config`).
pub fn config(link: &Shared) -> Result<Config, String> {
    let system = crate::work::ddm::system_name(link);
    match link.borrow_mut().with(|l| l.netsvr(NsOp::Config)) {
        NsReply::Files { cfg, shr } => {
            netsvrcfg::file::read(&cfg, &shr, &system).map_err(|e| e.to_string())
        }
        NsReply::Error { text } | NsReply::Refused { text, .. } => Err(text),
        other => Err(format!("{other:?}")),
    }
}

/// What one `QZLSFILE` serves with: the current attributes and every
/// share.
pub fn server_config(c: &Config) -> NetServerConfig {
    let a = &c.attrs.current;
    let idle = u64::try_from(a.idle_timeout).map_or(Duration::from_secs(u64::from(u32::MAX)), Duration::from_secs);
    let mut n = NetServerConfig {
        server_name: a.server_name.clone(),
        domain: a.domain.clone(),
        text: a.text.clone(),
        signing_required: a.signing_required(),
        encryption_required: a.encryption_required(),
        idle_timeout: idle,
        min_dialect: a.min_dialect.code(),
        guest: (!a.guest.is_empty()).then(|| a.guest.clone()),
        ..NetServerConfig::default()
    };
    for s in c.shares.values() {
        match &s.kind {
            Kind::File(f) => n.file_shares.push(FileShare {
                name: s.name.clone(),
                path: f.path.clone(),
                text: s.text.clone(),
                read_only: f.permission == Permission::ReadOnly,
                encrypt: s.encryption_required,
                leases: a.oplocks(),
            }),
            Kind::Print(p) => n.print_shares.push(PrintShareCfg {
                name: s.name.clone(),
                text: s.text.clone(),
                queue: OutQueue {
                    library: p.outq.library.clone(),
                    name: p.outq.name.clone(),
                },
                driver: p.driver.clone(),
            }),
        }
    }
    n
}

/// Message `msgid` of `QCPFMSG` to `QSYSOPR`, its data made from
/// `values` and its text from the message file.
pub fn sysopr(link: &Shared, msgid: &str, values: &[&str]) {
    let desc = link
        .borrow_mut()
        .with(|l| l.describe(msgid.to_string(), "QCPFMSG".into(), "*LIBL".into()));
    let (data, text, severity) = desc.map_or_else(
        || (Vec::new(), msgid.to_string(), 0),
        |d| {
            let data = d.data_of(values);
            let text = d.format(&data);
            (data, text, u8::try_from(d.severity).unwrap_or(99))
        },
    );
    let m = QMsg {
        kind: MsgKind::Info,
        severity,
        msgid: msgid.to_string(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        data,
        text,
        from_pgm: SERVER_PROGRAM.into(),
        ..QMsg::default()
    };
    if let Err(e) = link.borrow_mut().with(|l| l.msgq_send("*SYSOPR".into(), m)) {
        eprintln!("qsjob: NetServer: QSYSOPR: {e}");
    }
}

/// `QZLSSERVER`'s state: whether it was asked to end.
pub fn ending(link: &Shared) -> bool {
    matches!(
        link.borrow_mut().with(|l| l.netsvr(NsOp::State)),
        NsReply::State { ending: true, .. }
    )
}
