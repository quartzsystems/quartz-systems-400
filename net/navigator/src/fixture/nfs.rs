// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's NFS: three exports, an `/etc/exports` with one
//! more, the binder, two server jobs and the mount daemon running, and
//! six mounted file systems. `EXPORTFS`, `STRNFSSVR`, `ENDNFSSVR`,
//! `MOUNT` and `UNMOUNT` change them as the commands would.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::Message;

use super::cl::{done, fail, parse, unquote};
use super::{has_special, now};
use crate::env::nfs::{Daemon, Export, HostOption, Mount, Nfs};
use crate::routes::nfs::TYPES;

/// What the commands change.
#[derive(Debug, Default)]
pub struct State {
    /// The export table.
    pub exports: Vec<Export>,
    /// `/etc/exports`.
    pub file: Vec<Export>,
    /// The daemon jobs.
    pub daemons: Vec<Daemon>,
    /// The mount table.
    pub mounts: Vec<Mount>,
    next_job: u32,
}

/// The sample NFS.
pub struct FixtureNfs {
    /// Its state.
    pub state: Mutex<State>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

fn export(id: u32, path: &str, f: impl FnOnce(&mut Export)) -> Export {
    let mut e = Export {
        id,
        path: path.into(),
        versions: "3:4".into(),
        anon_uid: Some(4_294_770_710),
        anon_user: "QNFSANON".into(),
        by: "123390/QSECOFR/QPADEV0003".into(),
        since: now() - 86_400,
        ..Export::default()
    };
    f(&mut e);
    e
}

fn daemon(server: &str, n: u32, ports: &[&str]) -> Daemon {
    Daemon {
        server: server.into(),
        job: format!("{n:06}/QSECOFR/{}", job_name(server)),
        ready: true,
        ending: false,
        ports: ports.iter().map(ToString::to_string).collect(),
        since: now() - 7200,
    }
}

fn mount(id: u32, kind: &str, at: &str, f: impl FnOnce(&mut Mount)) -> Mount {
    let mut m = Mount {
        id,
        kind: kind.into(),
        at: at.into(),
        ..Mount::default()
    };
    f(&mut m);
    m
}

/// A daemon type's job name.
fn job_name(server: &str) -> &'static str {
    match server {
        "*RPC" => "QNFSRPCD",
        "*BIO" => "QNFSBIOD",
        "*RGY" => "QNFSRGYD",
        "*SVR" => "QNFSNFSD",
        "*MNT" => "QNFSMNTD",
        "*NSM" => "QNFSNSMD",
        _ => "QNFSNLMD",
    }
}

impl Default for FixtureNfs {
    fn default() -> Self {
        let home = export(1, "/home", |e| {
            e.read_only = true;
            e.rw_hosts = "admin01".into();
            e.hosts = vec![HostOption {
                host: "pc0412".into(),
                data: "819".into(),
                path: "819".into(),
                sync: false,
            }];
        });
        let docs = export(2, "/home/shared/docs", |e| {
            e.access_hosts = "@10.0.10.0/24".into();
            e.root_hosts = "admin01".into();
        });
        let data = export(3, "/QOpenSys/data", |e| {
            e.versions = "4".into();
            e.nosuid = true;
            e.pseudo_root = true;
            e.exname = "/data".into();
            e.anon = "-1".into();
            e.anon_uid = None;
            e.anon_user = "*NONE".into();
        });
        let www = export(0, "/www/public", |e| e.read_only = true);
        let state = State {
            exports: vec![home.clone(), docs, data],
            file: vec![home, www],
            daemons: vec![
                daemon("*RPC", 123_401, &["111/tcp", "111/udp"]),
                daemon("*SVR", 123_404, &["2049/tcp", "2049/udp"]),
                daemon("*SVR", 123_405, &[]),
                daemon("*MNT", 123_406, &["32769/tcp", "32769/udp"]),
            ],
            mounts: vec![
                mount(1, "root", "/", |_| {}),
                mount(2, "qopensys", "/QOpenSys", |_| {}),
                mount(5, "qsys", "/QSYS.LIB", |_| {}),
                mount(100, "udfs", "/home/projects", |m| {
                    m.source = "/dev/QASP01/PROJECTS.udfs".into();
                }),
                mount(101, "nfs3", "/mnt/build", |m| {
                    m.source = "build01:/export/artifacts".into();
                    m.options = "rw,hard,vers=3".into();
                    m.responding = Some(true);
                }),
                mount(102, "nfs4", "/mnt/lab", |m| {
                    m.source = "lab02:/srv".into();
                    m.options = "ro,soft,vers=4".into();
                    m.read_only = true;
                    m.responding = Some(false);
                }),
            ],
            next_job: 123_500,
        };
        Self {
            state: Mutex::new(state),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// `-O`'s options into `e`.
fn take_options(e: &mut Export, list: &str) {
    for o in list.split(',').filter(|o| !o.is_empty()) {
        let (k, v) = o.split_once('=').unwrap_or((o, ""));
        match k.to_ascii_lowercase().as_str() {
            "ro" => e.read_only = true,
            "rw" => e.rw_hosts = v.into(),
            "access" => e.access_hosts = v.into(),
            "root" if v.is_empty() => e.pseudo_root = true,
            "root" => e.root_hosts = v.into(),
            "anon" => e.anon = v.into(),
            "nosuid" => e.nosuid = true,
            "vers" => e.versions = v.into(),
            "public" => e.public = true,
            "nfsroot" => e.nfsroot = v.into(),
            "exname" => e.exname = v.into(),
            _ => {}
        }
    }
    if e.anon == "-1" {
        e.anon_uid = None;
        e.anon_user = "*NONE".into();
    } else if let Ok(u) = e.anon.parse() {
        e.anon_uid = Some(u);
        e.anon_user = "*NONE".into();
    }
}

/// `HOSTOPT`'s elements: `('host' data path write) ...`.
fn host_options(v: &str) -> Vec<HostOption> {
    v.split(')')
        .filter_map(|g| {
            let g = g.trim().trim_start_matches('(');
            let w: Vec<&str> = g.split_whitespace().collect();
            let host = unquote(w.first()?);
            Some(HostOption {
                host,
                data: w.get(1).copied().unwrap_or("*BINARY").into(),
                path: w.get(2).copied().unwrap_or("*ASCII").into(),
                sync: w.get(3).copied() != Some("*ASYNC"),
            })
        })
        .collect()
}

fn put(st: &mut State, mut e: Export) {
    if let Some(old) = st.exports.iter_mut().find(|x| x.path == e.path) {
        e.id = old.id;
        *old = e;
    } else {
        e.id = st.exports.iter().map(|x| x.id).max().unwrap_or(0) + 1;
        st.exports.push(e);
    }
}

fn exportfs(st: &mut State, p: &BTreeMap<String, String>) -> Result<Message, Message> {
    let flags = unquote(p.get("OPTIONS").map_or("", String::as_str));
    let dir = unquote(p.get("DIR").map_or("", String::as_str));
    let has = |f: &str| flags.split_whitespace().any(|w| w.eq_ignore_ascii_case(f));
    if has("-A") {
        let n = st.file.len();
        for e in st.file.clone() {
            put(st, e);
        }
        return done("CPCA1C6", &[&n.to_string(), "0"]);
    }
    if !dir.starts_with('/') {
        return fail("CPFA1C7", &[&dir]);
    }
    if has("-U") {
        let before = st.exports.len();
        st.exports.retain(|e| e.path != dir);
        if st.exports.len() == before {
            return fail("CPFA1C7", &[&dir]);
        }
        if has("-F") {
            st.file.retain(|e| e.path != dir);
        }
        return done("CPCA1C4", &[&dir]);
    }
    let mut e = export(0, &dir, |_| {});
    e.by = "123399/QSECOFR/NAVIGATOR".into();
    e.since = now();
    let words: Vec<&str> = flags.split_whitespace().collect();
    if let Some(i) = words.iter().position(|w| w.eq_ignore_ascii_case("-O")) {
        take_options(&mut e, words.get(i + 1).copied().unwrap_or(""));
    }
    e.hosts = host_options(p.get("HOSTOPT").map_or("", String::as_str));
    if has("-F") {
        st.file.retain(|x| x.path != dir);
        st.file.push(e.clone());
    }
    put(st, e);
    done("CPCA1C4", &[&dir])
}

fn servers(server: &str) -> Vec<&'static str> {
    TYPES
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| server == "*ALL" || *k == server)
        .collect()
}

fn run_in(st: &mut State, name: &str, p: &BTreeMap<String, String>) -> Result<Message, Message> {
    let val = |k: &str| unquote(p.get(k).map_or("", String::as_str));
    match name {
        "EXPORTFS" => exportfs(st, p),
        "STRNFSSVR" => {
            for k in servers(&val("SERVER")) {
                if !st.daemons.iter().any(|d| d.server == k) {
                    st.next_job += 1;
                    let mut d = daemon(k, st.next_job, &[]);
                    d.since = now();
                    st.daemons.push(d);
                }
            }
            done("CPCA1BB", &[])
        }
        "ENDNFSSVR" => {
            let list = servers(&val("SERVER"));
            st.daemons.retain(|d| !list.contains(&d.server.as_str()));
            done("CPCA1BC", &[])
        }
        "MOUNT" => {
            let (mfs, over) = (val("MFS"), val("MNTOVRDIR"));
            let options = val("OPTIONS");
            let kind = if options.contains("vers=4") {
                "nfs4"
            } else {
                "nfs3"
            };
            let id = st.mounts.iter().map(|m| m.id).max().unwrap_or(100) + 1;
            st.mounts.push(Mount {
                id,
                kind: kind.into(),
                at: over.clone(),
                source: mfs.clone(),
                read_only: options.split(',').any(|o| o == "ro"),
                options,
                responding: Some(true),
            });
            done("CPCA1B0", &[&mfs, &over])
        }
        "UNMOUNT" => {
            let over = val("MNTOVRDIR");
            let before = st.mounts.len();
            st.mounts.retain(|m| m.at != over);
            if st.mounts.len() == before {
                return fail("CPFA1C7", &[&over]);
            }
            done("CPCA1B1", &[&over])
        }
        _ => fail("CPD0030", &[name]),
    }
}

impl Nfs for FixtureNfs {
    fn exports(&self, _user: &str) -> Result<Vec<Export>, Message> {
        Ok(self.state.lock().expect("nfs").exports.clone())
    }

    fn daemons(&self, _user: &str) -> Vec<Daemon> {
        self.state.lock().expect("nfs").daemons.clone()
    }

    fn mounts(&self, _user: &str) -> Result<Vec<Mount>, Message> {
        Ok(self.state.lock().expect("nfs").mounts.clone())
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, p) = parse(command);
        if !has_special(user, "*IOSYSCFG") {
            return fail("CPFA1B8", &[&name]);
        }
        let mut st = self.state.lock().expect("nfs");
        run_in(&mut st, &name, &p)
    }
}
