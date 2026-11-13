// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's NFS (`Nfs`, version 11, draft): an export table,
//! an `/etc/exports`, the daemon registry, a mount table and the NFS
//! options in memory, changed by the five commands as the system's
//! would (only `SECOFR` has `*IOSYSCFG`, which each needs: `CPFA1B8`).
//!
//! The fixtures ([`shipped`]): `/home` exported read only with one host
//! option, `/www/public` in `/etc/exports` only; the binder and one
//! server job ready; root and one NFS mount.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use qap_proto::ops::nfs::{DaemonType, MountedFs, NfsOptions, Registration};

use super::EPOCH;
use crate::nfs::{DaemonEntry, ExportEntry};

/// The test system's NFS.
#[derive(Debug, Default)]
pub struct Nfs {
    /// The export table.
    pub exports: Vec<ExportEntry>,
    /// `/etc/exports`, one entry a line.
    pub file: Vec<String>,
    /// The daemon jobs.
    pub daemons: Vec<DaemonEntry>,
    /// The mount table.
    pub mounts: Vec<MountedFs>,
    /// The NFS options.
    pub options: NfsOptions,
    /// The commands run, as typed.
    pub ran: Vec<String>,
    next: u32,
}

fn daemon(d: DaemonType, n: u32, port: u16) -> DaemonEntry {
    DaemonEntry {
        daemon: d,
        job: format!("{n:06}/QSECOFR/{}", job_name(d)),
        ready: true,
        ending: false,
        registrations: if port == 0 {
            Vec::new()
        } else {
            vec![Registration {
                program: if d == DaemonType::Rpc {
                    100_000
                } else {
                    100_003
                },
                version: if d == DaemonType::Rpc { 4 } else { 3 },
                proto: "tcp".into(),
                port,
            }]
        },
        since: EPOCH - 600,
    }
}

fn job_name(d: DaemonType) -> &'static str {
    match d {
        DaemonType::Rpc => "QNFSRPCD",
        DaemonType::Bio => "QNFSBIOD",
        DaemonType::Rgy => "QNFSRGYD",
        DaemonType::Svr => "QNFSNFSD",
        DaemonType::Mnt => "QNFSMNTD",
        DaemonType::Nsm => "QNFSNSMD",
        DaemonType::Nlm => "QNFSNLMD",
    }
}

/// The fixtures.
#[must_use]
pub fn shipped() -> Nfs {
    let home = "/home -ro,rw=admin HOSTOPT pc1:819:819:*ASYNC";
    Nfs {
        exports: vec![ExportEntry {
            id: 1,
            entry: home.into(),
            anon_uid: Some(4_294_770_710),
            anon_user: "QNFSANON".into(),
            by: "000101/QSECOFR/QPADEV0001".into(),
            since: EPOCH - 3600,
        }],
        file: vec![home.into(), "/www/public -ro".into()],
        daemons: vec![
            daemon(DaemonType::Rpc, 300, 111),
            daemon(DaemonType::Svr, 301, 2049),
        ],
        mounts: vec![
            MountedFs {
                id: 1,
                kind: "root".into(),
                over: "/".into(),
                ..MountedFs::default()
            },
            MountedFs {
                id: 101,
                kind: "nfs3".into(),
                over: "/mnt/build".into(),
                source: "build01:/export".into(),
                options: "rw,hard".into(),
                read_only: false,
                responding: Some(true),
            },
        ],
        options: NfsOptions {
            flavors: vec![1],
            domain: String::new(),
            max_threads: 0,
            handle_cache: 1024,
            utf8_validation: true,
        },
        ran: Vec::new(),
        next: 400,
    }
}

/// A command's name and its `KEYWORD(value)` parameters as typed.
fn parse(line: &str) -> (String, BTreeMap<String, String>) {
    let (name, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    let mut parms = BTreeMap::new();
    let c: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < c.len() {
        let start = i;
        while i < c.len() && c[i] != '(' {
            i += 1;
        }
        let key: String = c[start..i].iter().collect::<String>().trim().to_string();
        let (mut depth, mut quoted, from) = (0, false, i + 1);
        while i < c.len() {
            match c[i] {
                '\'' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let v: String = c[from.min(c.len())..i.min(c.len())].iter().collect();
        parms.insert(key.to_ascii_uppercase(), v);
        i += 1;
    }
    (name.to_ascii_uppercase(), parms)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    s.strip_prefix('\'')
        .and_then(|r| r.strip_suffix('\''))
        .map_or_else(|| s.to_string(), |t| t.replace("''", "'"))
}

/// `HOSTOPT((host data path write) ...)` as `/etc/exports` writes it.
fn hostopts(v: &str) -> String {
    let mut s = String::new();
    for g in v.split(')') {
        let w: Vec<&str> = g
            .trim()
            .trim_start_matches('(')
            .split_whitespace()
            .collect();
        if let Some(h) = w.first() {
            let _ = write!(s, " HOSTOPT {}:{}", unquote(h), w[1..].join(":"));
        }
    }
    s
}

impl Nfs {
    /// Run `line`: `Err` with the escape message's ID and data.
    ///
    /// # Errors
    /// The command's escape message.
    pub fn run(&mut self, line: &str, me: &str) -> Result<(), (String, Vec<String>)> {
        self.ran.push(line.to_string());
        let (name, p) = parse(line);
        let v = |k: &str| unquote(p.get(k).map_or("", String::as_str));
        let fail = |id: &str, d: &[&str]| {
            Err((id.to_string(), d.iter().map(ToString::to_string).collect()))
        };
        match name.as_str() {
            "EXPORTFS" => {
                let (flags, dir) = (v("OPTIONS"), v("DIR"));
                let words: Vec<&str> = flags.split_whitespace().collect();
                let has = |f: &str| words.iter().any(|w| w.eq_ignore_ascii_case(f));
                if has("-A") {
                    for line in self.file.clone() {
                        self.put(&line, me);
                    }
                    return Ok(());
                }
                if has("-U") {
                    let before = self.exports.len();
                    self.exports.retain(|e| {
                        !nfs::exports::file::entry(1, &e.entry)
                            .is_ok_and(|x| nfs::exports::file::same_path(&x.path, &dir))
                    });
                    if self.exports.len() == before {
                        return fail("CPFA1C7", &[&dir]);
                    }
                    return Ok(());
                }
                let mut line = dir.clone();
                if let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("-O")) {
                    let _ = write!(line, " -{}", words.get(i + 1).copied().unwrap_or(""));
                }
                line.push_str(&hostopts(p.get("HOSTOPT").map_or("", String::as_str)));
                if nfs::exports::file::entry(1, &line).is_err() {
                    return fail("CPFA1C7", &[&dir]);
                }
                if has("-F") {
                    self.file.push(line.clone());
                }
                self.put(&line, me);
                Ok(())
            }
            "STRNFSSVR" | "ENDNFSSVR" => {
                let s = v("SERVER");
                let types: Vec<DaemonType> = DaemonType::ALL
                    .into_iter()
                    .filter(|d| s == "*ALL" || d.keyword() == s)
                    .collect();
                if name == "ENDNFSSVR" {
                    self.daemons.retain(|d| !types.contains(&d.daemon));
                } else {
                    for d in types {
                        if !self.daemons.iter().any(|x| x.daemon == d) {
                            self.next += 1;
                            self.daemons.push(daemon(d, self.next, 0));
                        }
                    }
                }
                Ok(())
            }
            "MOUNT" => {
                let id = self.mounts.iter().map(|m| m.id).max().unwrap_or(100) + 1;
                self.mounts.push(MountedFs {
                    id,
                    kind: "nfs3".into(),
                    over: v("MNTOVRDIR"),
                    source: v("MFS"),
                    options: v("OPTIONS"),
                    read_only: false,
                    responding: Some(true),
                });
                Ok(())
            }
            "UNMOUNT" => {
                let over = v("MNTOVRDIR");
                let before = self.mounts.len();
                self.mounts.retain(|m| m.over != over);
                if self.mounts.len() == before {
                    return fail("CPFA1C7", &[&over]);
                }
                Ok(())
            }
            _ => fail("CPD0030", &[&name, "*LIBL"]),
        }
    }

    fn put(&mut self, line: &str, me: &str) {
        let Ok(x) = nfs::exports::file::entry(1, line) else {
            return;
        };
        let by = format!("000200/{me}/QAPCNN");
        if let Some(e) = self.exports.iter_mut().find(|e| {
            nfs::exports::file::entry(1, &e.entry)
                .is_ok_and(|o| nfs::exports::file::same_path(&o.path, &x.path))
        }) {
            e.entry = line.to_string();
            e.by = by;
            return;
        }
        let id = self.exports.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        self.exports.push(ExportEntry {
            id,
            entry: line.to_string(),
            anon_uid: Some(4_294_770_710),
            anon_user: "QNFSANON".into(),
            by,
            since: EPOCH,
        });
    }
}
