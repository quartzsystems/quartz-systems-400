// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::nfs::Nfs` as the user: the export table and the
//! daemon registry through the storage service's `nfs` request (the
//! same `EXPORTFS` and `STRNFSSVR` use), each entry read with
//! `nfs::exports::file`; every change the command itself through the
//! analyzer ([`Link::run`]).

use navigator::env::nfs::{Daemon, Export, HostOption, Mount, Nfs};
use nfs::exports::options::print_hosts;
use nfs::exports::{DataCcsid, PathCcsid};
use qsweb::msg::{Kind, Message};
use stg::Client as _;
use stg::nfs::{NfsDaemonJob, NfsExport, NfsOp, NfsReply};

use super::Link;
use crate::work::web::bridge::Main;

/// NFS.
pub struct LiveNfs {
    /// The job's thread.
    pub link: Link,
}

fn ask(m: &Main, op: NfsOp) -> NfsReply {
    m.ask(|l| l.nfs(op))
}

/// An entry of the table as the pages show it; `None` when its line
/// does not read (it always does: the service printed it).
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn export(e: &NfsExport) -> Option<Export> {
    let x = nfs::exports::file::entry(1, &e.entry).ok()?;
    let o = &x.options;
    let v = o.versions();
    let versions = match (v.v3, v.v4) {
        (true, false) => "3",
        (false, true) => "4",
        _ => "3:4",
    };
    Some(Export {
        id: e.id,
        path: x.path.clone(),
        read_only: o.ro,
        rw_hosts: o.rw.as_deref().map(print_hosts).unwrap_or_default(),
        access_hosts: o.access.as_deref().map(print_hosts).unwrap_or_default(),
        root_hosts: print_hosts(&o.root_hosts),
        anon: o.anon.map(|a| a.to_string()).unwrap_or_default(),
        anon_uid: e.anon_uid,
        anon_user: e.anon_profile.clone(),
        nosuid: o.nosuid,
        versions: versions.into(),
        public: o.public,
        pseudo_root: o.pseudo_root,
        nfsroot: o.nfsroot.clone().unwrap_or_default(),
        exname: o.exname.clone().unwrap_or_default(),
        hosts: x
            .hosts
            .iter()
            .map(|h| HostOption {
                host: print_hosts(std::slice::from_ref(&h.host)),
                data: match h.data {
                    DataCcsid::Ccsid(n) => n.to_string(),
                    DataCcsid::Ascii => "*ASCII".into(),
                    DataCcsid::Job => "*JOBCCSID".into(),
                    DataCcsid::Binary => "*BINARY".into(),
                },
                path: match h.path {
                    PathCcsid::Ccsid(n) => n.to_string(),
                    PathCcsid::Ascii => "*ASCII".into(),
                    PathCcsid::Job => "*JOBCCSID".into(),
                },
                sync: h.sync,
            })
            .collect(),
        by: e.by.clone(),
        since: e.since,
    })
}

/// A daemon job as the pages show it.
#[must_use]
pub fn daemon(j: &NfsDaemonJob) -> Daemon {
    let mut ports: Vec<String> = j
        .ports
        .iter()
        .map(|p| format!("{}/{}", p.port, p.proto))
        .collect();
    ports.dedup();
    Daemon {
        server: j.daemon.keyword().into(),
        job: j.job.clone(),
        ready: j.ready,
        ending: j.ending,
        ports,
        since: j.since,
    }
}

/// Message `id` of `file`, its text from the message file.
fn described(m: &Main, id: &str, file: &str) -> Message {
    let (i, f) = (id.to_string(), file.to_string());
    let text = m
        .ask(|l| l.describe(i, f, "*LIBL".into()))
        .map_or_else(|| id.to_string(), |d| d.text);
    Message::new(Kind::Error, id, &text)
}

/// The mount table, as the signed-on user.
// BLOCKED(rel): the IFS mount table (NFS-4): `IfsClient::mounts()` over
// the job's IFS client, each `MountInfo` as a `Mount`; until then
// `TCP1A78`.
fn mounts(m: &Main) -> Result<Vec<Mount>, Message> {
    Err(described(m, "TCP1A78", "QTCPMSG"))
}

fn exports(m: &Main) -> Result<Vec<Export>, Message> {
    match ask(m, NfsOp::Exports) {
        NfsReply::Table { exports, .. } => Ok(exports.iter().filter_map(export).collect()),
        NfsReply::Refused { msgid, text, .. } => Err(Message::new(Kind::Error, &msgid, &text)),
        _ => Ok(Vec::new()),
    }
}

impl LiveNfs {
    /// `f` on the job's thread as `user`.
    fn as_user<T: Send + 'static>(
        &self,
        user: &str,
        f: impl FnOnce(&mut Main) -> Result<T, Message> + Send + 'static,
    ) -> Result<T, Message> {
        let u = user.to_string();
        self.link
            .bridge
            .call(move |m| m.as_user(&u, f))
            .unwrap_or_else(|| Err(super::gone()))
            .and_then(|r| r)
    }
}

impl Nfs for LiveNfs {
    fn exports(&self, user: &str) -> Result<Vec<Export>, Message> {
        self.as_user(user, |m| exports(m))
    }

    fn daemons(&self, user: &str) -> Vec<Daemon> {
        self.link.read(user, |m| match ask(m, NfsOp::Daemons) {
            NfsReply::Daemons { jobs } => jobs.iter().map(daemon).collect(),
            _ => Vec::new(),
        })
    }

    fn mounts(&self, user: &str) -> Result<Vec<Mount>, Message> {
        self.as_user(user, |m| mounts(m))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_as_the_pages_show_it() {
        let e = NfsExport {
            id: 3,
            entry: "/home -ro,rw=a:@10.0.0.0/8,anon=-1,vers=4,root HOSTOPT pc1:819:1208:*ASYNC"
                .into(),
            anon_uid: None,
            anon_profile: "*NONE".into(),
            by: "j".into(),
            since: 9,
        };
        let x = export(&e).expect("reads");
        assert_eq!(x.rw_hosts, "a:@10.0.0.0/8");
        assert_eq!((x.anon.as_str(), x.versions.as_str()), ("-1", "4"));
        assert!(x.read_only && x.pseudo_root);
        assert_eq!(
            x.hosts,
            [HostOption {
                host: "pc1".into(),
                data: "819".into(),
                path: "1208".into(),
                sync: false,
            }]
        );
    }
}
