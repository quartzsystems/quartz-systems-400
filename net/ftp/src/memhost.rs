// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A [`Host`] in memory, for tests and the transcript replay: the IFS
//! over a `MemBackend` with an in-memory `/QSYS.LIB`, profiles with
//! passwords, the few CL commands FTP sessions run (files, members,
//! libraries, save files, data areas, the library list, overrides,
//! `CHKOBJ`), exit programs as a closure, a fixed clock.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ifs::Result as IfsResult;
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, LocalIfs};
use ifs::ops::audit::NoAudit;
use ifs::ops::{Config as IfsConfig, Ifs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData, QsysEntry, QsysNode, QsysView};
use regfac::{ExitAnswer, ExitRequest};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;

use crate::host::{CmdFail, DbFile, Host, Profile, Refused};

/// File descriptions by library and file, with their member limit.
type FileTable = BTreeMap<(String, String), (DbFile, Option<usize>)>;
/// The table shared with the IFS view.
type Files = Arc<Mutex<FileTable>>;

/// When objects and members last changed, nanoseconds, by
/// `LIB/NAME.TYPE` or `LIB/FILE/MBR`.
type Times = Arc<Mutex<BTreeMap<String, i64>>>;

fn now_nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

fn stamp(times: &Times, key: String) {
    times
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, now_nanos());
}

fn stamped(times: &Times, key: &str, mut e: QsysEntry) -> QsysEntry {
    if let Some(t) = times
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(key)
    {
        e.changed = *t;
        if e.created == 0 {
            e.created = *t;
        }
    }
    e
}

/// `/QSYS.LIB` shared between the IFS and the command interpreter; a
/// member made through the IFS takes its file's shape, and changes are
/// dated.
struct Shared {
    qsys: Arc<MemQsys>,
    files: Files,
    times: Times,
}

impl QsysView for Shared {
    fn libraries(&self) -> IfsResult<Vec<QsysEntry>> {
        self.qsys.libraries()
    }
    fn objects(&self, lib: &str) -> IfsResult<Vec<QsysEntry>> {
        Ok(self
            .qsys
            .objects(lib)?
            .into_iter()
            .map(|e| {
                let k = format!("{lib}/{}.{}", e.name, e.objtype);
                stamped(&self.times, &k, e)
            })
            .collect())
    }
    fn object(&self, lib: &str, name: &str, objtype: &str) -> IfsResult<QsysEntry> {
        let e = self.qsys.object(lib, name, objtype)?;
        Ok(stamped(&self.times, &format!("{lib}/{name}.{objtype}"), e))
    }
    fn members(&self, lib: &str, file: &str) -> IfsResult<Vec<QsysEntry>> {
        Ok(self
            .qsys
            .members(lib, file)?
            .into_iter()
            .map(|e| {
                let k = format!("{lib}/{file}/{}", e.name);
                stamped(&self.times, &k, e)
            })
            .collect())
    }
    fn read_member(&self, lib: &str, file: &str, mbr: &str) -> IfsResult<MemberData> {
        self.qsys.read_member(lib, file, mbr)
    }
    fn write_member(&self, lib: &str, file: &str, mbr: &str, records: Vec<Vec<u8>>) -> IfsResult<()> {
        self.qsys.write_member(lib, file, mbr, records)?;
        stamp(&self.times, format!("{lib}/{file}/{mbr}"));
        Ok(())
    }
    fn read_stream(&self, lib: &str, name: &str, objtype: &str) -> IfsResult<Vec<u8>> {
        self.qsys.read_stream(lib, name, objtype)
    }
    fn write_stream(&self, lib: &str, name: &str, objtype: &str, data: &[u8]) -> IfsResult<()> {
        self.qsys.write_stream(lib, name, objtype, data)
    }
    fn authority(&self, lib: &str, name: &str, objtype: &str, who: &Principal) -> IfsResult<Auth> {
        self.qsys.authority(lib, name, objtype, who)
    }
    fn create_library(&self, name: &str, who: &Principal) -> IfsResult<()> {
        self.qsys.create_library(name, who)
    }
    fn create_member(&self, lib: &str, file: &str, mbr: &str, who: &Principal) -> IfsResult<()> {
        let desc = self
            .files
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&(lib.to_string(), file.to_string()))
            .map(|(d, _)| d.clone());
        match desc {
            Some(d) => {
                if self.qsys.read_member(lib, file, mbr).is_ok() {
                    return Err(ifs::Errno::EEXIST);
                }
                self.qsys.add_member(
                    lib,
                    file,
                    mbr,
                    MemberData {
                        records: Vec::new(),
                        record_len: d.rec_len,
                        source: d.source,
                        ccsid: d.ccsid,
                    },
                );
                stamp(&self.times, format!("{lib}/{file}/{mbr}"));
                Ok(())
            }
            None => self.qsys.create_member(lib, file, mbr, who),
        }
    }
    fn delete(&self, node: &QsysNode, who: &Principal) -> IfsResult<()> {
        self.qsys.delete(node, who)
    }
}

/// An exit program: the point, the request, the answer.
pub type ExitFn = Box<dyn FnMut(&str, &ExitRequest) -> Option<ExitAnswer>>;

/// The host in memory.
pub struct MemHost {
    /// The IFS as the signed-on user.
    pub ifs: LocalIfs<MemBackend>,
    /// `/QSYS.LIB`.
    pub qsys: Arc<MemQsys>,
    /// Profiles: name, password, home, current library.
    pub users: BTreeMap<String, (String, Profile)>,
    files: Files,
    times: Times,
    /// Every CL command run, in order.
    pub commands: Vec<String>,
    /// The exit programs.
    pub exits: Option<ExitFn>,
    /// Now, Unix seconds.
    pub clock: i64,
    /// The local time's offset.
    pub offset: i64,
    /// Who the session runs as.
    pub current: String,
    /// The job's CCSID.
    pub ccsid: u16,
    /// The user part of the library list.
    pub libl: Vec<String>,
    /// File overrides by name.
    pub overrides: Vec<String>,
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = sec::profile::spcaut::SpecialAuth::ALLOBJ.0;
    }
    p
}

/// `KWD(value)` of a command string.
fn kwd(cmd: &str, k: &str) -> Option<String> {
    servers::attrs::keyword(cmd, k).map(|v| v.trim().trim_matches('\'').to_string())
}

/// `LIB/NAME` (the library `*LIBL`, `*CURLIB` or missing is `cur`).
fn qual(v: &str, cur: &str) -> (String, String) {
    match v.split_once('/') {
        Some((l, n)) if !l.starts_with('*') => (l.to_ascii_uppercase(), n.to_ascii_uppercase()),
        Some((_, n)) => (cur.to_string(), n.to_ascii_uppercase()),
        None => (cur.to_string(), v.to_ascii_uppercase()),
    }
}

fn fail(msgid: &str, text: impl Into<String>) -> CmdFail {
    CmdFail {
        msgid: msgid.into(),
        text: text.into(),
    }
}

impl MemHost {
    /// An empty system: `QSYS`, `QGPL`, `/home`, the profiles given as
    /// (name, password), each with `/home/NAME`.
    pub fn new(users: &[(&str, &str)]) -> Self {
        let qsys = Arc::new(MemQsys::new(Auth::USE));
        qsys.add_lib("QGPL");
        let files: Files = Arc::default();
        let times: Times = Arc::default();
        let mut env = MemEnv::default().user("QSYS", 0).user("QSECOFR", 1).user("QDFTOWN", 3);
        for (i, (u, _)) in users.iter().enumerate() {
            env = env.user(u, 100 + u32::try_from(i).unwrap_or(0));
        }
        let view = Shared {
            qsys: Arc::clone(&qsys),
            files: Arc::clone(&files),
            times: Arc::clone(&times),
        };
        let ifs = Ifs::new(MemBackend::new(), Box::new(view), IfsConfig::default());
        ifs.ensure_layout(&env).expect("layout");
        let local = LocalIfs {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(NoAudit),
            principal: principal("QSECOFR"),
            owner: Owner(1),
            job_ccsid: 37,
        };
        let mut map = BTreeMap::new();
        for (i, (u, pw)) in users.iter().enumerate() {
            let home = format!("/home/{u}");
            let _ = local.mkdir("/home", 0o755);
            let _ = local.mkdir(&home, 0o770);
            let _ = local.chown(&home, 100 + u32::try_from(i).unwrap_or(0), 0);
            map.insert(
                (*u).to_string(),
                (
                    (*pw).to_string(),
                    Profile {
                        user: (*u).to_string(),
                        home,
                        curlib: "*CRTDFT".into(),
                        ccsid: 37,
                    },
                ),
            );
        }
        Self {
            ifs: local,
            qsys,
            users: map,
            files,
            times,
            commands: Vec::new(),
            exits: None,
            clock: 1_793_309_580,
            offset: 0,
            current: "QSECOFR".into(),
            ccsid: 37,
            libl: vec!["QGPL".into(), "QTEMP".into()],
            overrides: Vec::new(),
        }
    }

    /// Run as `user` from now on (setting fixtures up as the user).
    pub fn act_as(&mut self, user: &str) {
        self.current = user.to_string();
        self.ifs.principal = principal(user);
    }

    /// Add a library.
    pub fn add_library(&mut self, lib: &str) {
        self.qsys.add_lib(lib);
    }

    fn files(&self) -> std::sync::MutexGuard<'_, FileTable> {
        self.files.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Add a physical file and its description; `rec_len` is the data
    /// part for a source file.
    pub fn add_file(&mut self, lib: &str, file: &str, rec_len: usize, source: bool, ccsid: u16, owner: &str) {
        self.add_file_max(lib, file, rec_len, source, ccsid, owner, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn add_file_max(
        &mut self,
        lib: &str,
        file: &str,
        rec_len: usize,
        source: bool,
        ccsid: u16,
        owner: &str,
        max: Option<usize>,
    ) {
        let attr = if source { "PF-SRC" } else { "PF-DTA" };
        self.qsys.add_obj(lib, file, "*FILE", attr, owner);
        stamp(&self.times, format!("{lib}/{file}.*FILE"));
        self.files().insert(
            (lib.into(), file.into()),
            (
                DbFile {
                    rec_len,
                    source,
                    ccsid,
                    logical: false,
                },
                max,
            ),
        );
    }

    /// Add a member with text lines (padded to the record length).
    pub fn add_member(&mut self, lib: &str, file: &str, mbr: &str, lines: &[&str]) {
        let Some((d, _)) = self.files().get(&(lib.to_string(), file.to_string())).cloned() else {
            return;
        };
        let records = lines
            .iter()
            .map(|l| {
                let mut r = ccsid::conv::from_str(d.ccsid, l).unwrap_or_default();
                r.resize(d.rec_len, 0x40);
                r.truncate(d.rec_len);
                r
            })
            .collect();
        self.qsys.add_member(
            lib,
            file,
            mbr,
            MemberData {
                records,
                record_len: d.rec_len,
                source: d.source,
                ccsid: d.ccsid,
            },
        );
        stamp(&self.times, format!("{lib}/{file}/{mbr}"));
    }

    /// The text of a member's records, trailing blanks dropped.
    pub fn member_text(&self, lib: &str, file: &str, mbr: &str) -> Option<Vec<String>> {
        let m = self.qsys.read_member(lib, file, mbr).ok()?;
        Some(
            m.records
                .iter()
                .map(|r| ccsid::conv::to_string(m.ccsid, r).unwrap_or_default().trim_end().to_string())
                .collect(),
        )
    }

    /// `LIB/NAME` of a type, `*LIBL` searched (the current library,
    /// then the library list).
    fn find(&self, v: &str, objtype: &str) -> (String, String) {
        let (l, n) = qual(v, &self.cur());
        let libl = v.split_once('/').is_none_or(|(l, _)| l.eq_ignore_ascii_case("*LIBL"));
        if libl {
            let mut search = vec![self.cur()];
            search.extend(self.libl.iter().cloned());
            if let Some(found) = search.into_iter().find(|lib| self.qsys.object(lib, &n, objtype).is_ok()) {
                return (found, n);
            }
            return ("*LIBL".into(), n);
        }
        (l, n)
    }

    fn cur(&self) -> String {
        self.users
            .get(&self.current)
            .map(|(_, p)| p.curlib.clone())
            .filter(|c| !c.starts_with('*'))
            .unwrap_or_else(|| "QGPL".into())
    }

    fn lib_exists(&self, lib: &str) -> bool {
        lib == "QSYS" || self.qsys.libraries().is_ok_and(|l| l.iter().any(|e| e.name == lib))
    }

    fn file_exists(&self, lib: &str, file: &str) -> bool {
        self.qsys.object(lib, file, "*FILE").is_ok()
    }

    fn create_file(&mut self, cmd: &str, source: bool) -> Result<(), CmdFail> {
        let (lib, file) = qual(&kwd(cmd, "FILE").ok_or_else(|| fail("CPD0071", "FILE is needed."))?, &self.cur());
        if !self.lib_exists(&lib) || self.file_exists(&lib, &file) {
            return Err(fail("CPF7302", format!("File {file} not created in library {lib}.")));
        }
        let dft = if source { 92 } else { 80 };
        let len: usize = kwd(cmd, "RCDLEN").and_then(|v| v.parse().ok()).unwrap_or(dft);
        let len = if source { len.saturating_sub(12).max(1) } else { len };
        let ccsid = kwd(cmd, "CCSID").and_then(|v| v.parse().ok()).unwrap_or(self.ccsid);
        let max = match kwd(cmd, "MAXMBRS").as_deref() {
            Some("*NOMAX") => None,
            Some(n) => n.parse().ok(),
            None if source => None,
            None => Some(1),
        };
        let owner = self.current.clone();
        self.add_file_max(&lib, &file, len, source, ccsid, &owner, max);
        let mbr = kwd(cmd, "MBR").map(|m| m.to_ascii_uppercase());
        match mbr.as_deref() {
            Some("*FILE") | None if !source => self.add_member(&lib, &file, &file, &[]),
            Some("*NONE" | "*FILE") | None => {}
            Some(m) => self.add_member(&lib, &file, m, &[]),
        }
        Ok(())
    }

    /// The few commands sessions use.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    fn run(&mut self, cmd: &str) -> Result<(), CmdFail> {
        let name = cmd.split([' ', '(']).next().unwrap_or("").to_ascii_uppercase();
        let cur = self.cur();
        match name.as_str() {
            "CRTPF" => self.create_file(cmd, false),
            "CRTSRCPF" => self.create_file(cmd, true),
            "ADDPFM" => {
                let (l, f) = qual(&kwd(cmd, "FILE").unwrap_or_default(), &cur);
                let m = kwd(cmd, "MBR").unwrap_or_default().to_ascii_uppercase();
                if !self.file_exists(&l, &f) {
                    return Err(fail("CPF7306", format!("Member {m} not added to file {f} in {l}.")));
                }
                let count = self.qsys.members(&l, &f).map_or(0, |v| v.len());
                let max = self.files().get(&(l.clone(), f.clone())).and_then(|(_, m)| *m);
                if self.qsys.read_member(&l, &f, &m).is_ok() || max.is_some_and(|x| count >= x) {
                    return Err(fail("CPF7306", format!("Member {m} not added to file {f} in {l}.")));
                }
                self.add_member(&l, &f, &m, &[]);
                Ok(())
            }
            "RMVM" | "CLRPFM" => {
                let (l, f) = qual(&kwd(cmd, "FILE").unwrap_or_default(), &cur);
                let m = kwd(cmd, "MBR").unwrap_or_else(|| f.clone()).to_ascii_uppercase();
                if self.qsys.read_member(&l, &f, &m).is_err() {
                    return Err(fail("CPF3141", format!("Member {m} not found in file {f} in {l}.")));
                }
                if name == "RMVM" {
                    let _ = self.qsys.delete(&QsysNode::Mbr(l, f, m), &principal("QSECOFR"));
                } else {
                    let _ = self.qsys.write_member(&l, &f, &m, Vec::new());
                }
                Ok(())
            }
            "DLTF" => {
                let (l, f) = qual(&kwd(cmd, "FILE").unwrap_or_default(), &cur);
                let p = principal("QSECOFR");
                let gone = self.qsys.delete(&QsysNode::Obj(l.clone(), f.clone(), "*FILE".into()), &p).is_ok()
                    || self.qsys.delete(&QsysNode::Obj(l.clone(), f.clone(), "*SAVF".into()), &p).is_ok();
                self.files().remove(&(l.clone(), f.clone()));
                if gone {
                    Ok(())
                } else {
                    Err(fail("CPF2105", format!("Object {f} in {l} type *FILE not found.")))
                }
            }
            "RNMM" => {
                let (l, f) = qual(&kwd(cmd, "FILE").unwrap_or_default(), &cur);
                let m = kwd(cmd, "MBR").unwrap_or_default().to_ascii_uppercase();
                let n = kwd(cmd, "NEWMBR").unwrap_or_default().to_ascii_uppercase();
                let data = self
                    .qsys
                    .read_member(&l, &f, &m)
                    .map_err(|_| fail("CPF3C27", format!("Member {m} not renamed.")))?;
                if self.qsys.read_member(&l, &f, &n).is_ok() {
                    return Err(fail("CPF3C27", format!("Member {m} not renamed.")));
                }
                let _ = self.qsys.delete(&QsysNode::Mbr(l.clone(), f.clone(), m), &principal("QSECOFR"));
                self.qsys.add_member(&l, &f, &n, data);
                Ok(())
            }
            "RNMOBJ" => {
                let (l, o) = qual(&kwd(cmd, "OBJ").unwrap_or_default(), &cur);
                let t = kwd(cmd, "OBJTYPE").unwrap_or_default().to_ascii_uppercase();
                let n = kwd(cmd, "NEWOBJ").unwrap_or_default().to_ascii_uppercase();
                if t != "*FILE" || !self.file_exists(&l, &o) || self.file_exists(&l, &n) {
                    return Err(fail("CPF2105", format!("Object {o} in {l} type {t} not renamed.")));
                }
                let members: Vec<(String, MemberData)> = self
                    .qsys
                    .members(&l, &o)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|e| self.qsys.read_member(&l, &o, &e.name).ok().map(|d| (e.name, d)))
                    .collect();
                let (d, max) = self.files().remove(&(l.clone(), o.clone())).unwrap_or((
                    DbFile {
                        rec_len: 80,
                        source: false,
                        ccsid: 37,
                        logical: false,
                    },
                    None,
                ));
                let _ = self.qsys.delete(&QsysNode::Obj(l.clone(), o, "*FILE".into()), &principal("QSECOFR"));
                let owner = self.current.clone();
                self.add_file_max(&l, &n, d.rec_len, d.source, d.ccsid, &owner, max);
                for (m, data) in members {
                    self.qsys.add_member(&l, &n, &m, data);
                }
                Ok(())
            }
            "CRTSAVF" => {
                let (l, f) = qual(&kwd(cmd, "FILE").unwrap_or_default(), &cur);
                self.qsys.add_obj(&l, &f, "*SAVF", "SAVF", &self.current.clone());
                stamp(&self.times, format!("{l}/{f}.*SAVF"));
                Ok(())
            }
            "CRTDTAARA" => {
                let (l, d) = qual(&kwd(cmd, "DTAARA").unwrap_or_default(), &cur);
                if self.qsys.object(&l, &d, "*DTAARA").is_ok() {
                    return Err(fail("CPF1023", format!("Data area {d} exists in {l}.")));
                }
                self.qsys.add_obj(&l, &d, "*DTAARA", "", &self.current.clone());
                stamp(&self.times, format!("{l}/{d}.*DTAARA"));
                Ok(())
            }
            "CHGDTAARA" | "DLTDTAARA" => {
                let v = kwd(cmd, "DTAARA").unwrap_or_default();
                let (l, d) = qual(v.split_whitespace().next().unwrap_or(""), &cur);
                if self.qsys.object(&l, &d, "*DTAARA").is_err() {
                    return Err(fail("CPF1015", format!("Data area {d} in {l} not found.")));
                }
                if name == "DLTDTAARA" {
                    let _ = self.qsys.delete(&QsysNode::Obj(l, d, "*DTAARA".into()), &principal("QSECOFR"));
                }
                Ok(())
            }
            "CRTLIB" => {
                let l = kwd(cmd, "LIB").unwrap_or_default().to_ascii_uppercase();
                if self.lib_exists(&l) {
                    return Err(fail("CPF2111", format!("Library {l} already exists.")));
                }
                self.qsys.add_lib(&l);
                Ok(())
            }
            "DLTLIB" => {
                let l = kwd(cmd, "LIB").unwrap_or_default().to_ascii_uppercase();
                self.qsys
                    .delete(&QsysNode::Lib(l.clone()), &principal("QSECOFR"))
                    .map_err(|_| fail("CPF2110", format!("Library {l} not found.")))
            }
            "CHKOBJ" => {
                let Some(o) = kwd(cmd, "OBJ") else {
                    return Err(fail("CPD0071", "OBJ is needed."));
                };
                let t = kwd(cmd, "OBJTYPE").unwrap_or_default().to_ascii_uppercase();
                let (l, n) = self.find(&o, &t);
                let found = if t == "*LIB" {
                    self.lib_exists(&n)
                } else {
                    self.qsys.object(&l, &n, &t).is_ok()
                };
                if !found {
                    return Err(fail("CPF9801", format!("Object {n} in library {l} not found.")));
                }
                if let Some(m) = kwd(cmd, "MBR")
                    && self.qsys.read_member(&l, &n, &m.to_ascii_uppercase()).is_err()
                {
                    return Err(fail("CPF9815", format!("Member {m} of file {n} in library {l} not found.")));
                }
                Ok(())
            }
            "CHGCURLIB" => {
                let l = kwd(cmd, "CURLIB").unwrap_or_default().to_ascii_uppercase();
                if !self.lib_exists(&l) {
                    return Err(fail("CPF2110", format!("Library {l} not found.")));
                }
                if let Some((_, p)) = self.users.get_mut(&self.current) {
                    p.curlib = l;
                }
                Ok(())
            }
            "ADDLIBLE" => {
                let l = kwd(cmd, "LIB").unwrap_or_default().to_ascii_uppercase();
                if !self.lib_exists(&l) {
                    return Err(fail("CPF2110", format!("Library {l} not found.")));
                }
                if self.libl.contains(&l) {
                    return Err(fail("CPF2103", format!("Library {l} is in the library list already.")));
                }
                self.libl.insert(0, l);
                Ok(())
            }
            "RMVLIBLE" => {
                let l = kwd(cmd, "LIB").unwrap_or_default().to_ascii_uppercase();
                match self.libl.iter().position(|x| *x == l) {
                    Some(i) => {
                        self.libl.remove(i);
                        Ok(())
                    }
                    None => Err(fail("CPF2104", format!("Library {l} was not removed from the library list."))),
                }
            }
            "OVRDBF" => {
                self.overrides.push(kwd(cmd, "FILE").unwrap_or_default().to_ascii_uppercase());
                Ok(())
            }
            "DLTOVR" => {
                let f = kwd(cmd, "FILE").unwrap_or_default().to_ascii_uppercase();
                match self.overrides.iter().position(|x| *x == f) {
                    Some(i) => {
                        self.overrides.remove(i);
                        Ok(())
                    }
                    None => Err(fail("CPF9841", format!("No override for file {f}."))),
                }
            }
            // A program message needs a program queue an FTP job does
            // not give it.
            "SNDPGMMSG" => Err(fail("CPF0001", "SNDPGMMSG failed.")),
            "DLYJOB" | "CHGJOB" | "DSPLIBL" | "WRKACTJOB" => Ok(()),
            // A comment alone does nothing.
            "/*" if cmd.trim_end().ends_with("*/") => Ok(()),
            _ => Err(fail("CPD0030", format!("Command {name} in library *LIBL not found."))),
        }
    }
}

impl Host for MemHost {
    fn ifs(&self) -> &dyn IfsClient {
        &self.ifs
    }

    fn sign_on(&mut self, user: &str, password: &str) -> Result<String, Refused> {
        let u = user.trim().to_ascii_uppercase();
        match self.users.get(&u) {
            Some((pw, _)) if pw == password => Ok(u),
            _ => Err(Refused {
                msgid: "CPF1107".into(),
                text: "Password or user profile is not correct.".into(),
            }),
        }
    }

    fn profile(&mut self, user: &str) -> Option<Profile> {
        self.users.get(&user.to_ascii_uppercase()).map(|(_, p)| p.clone())
    }

    fn become_user(&mut self, user: &str, _password: Option<&str>) -> Result<(), String> {
        self.act_as(&user.to_ascii_uppercase());
        Ok(())
    }

    fn command(&mut self, line: &str) -> Result<(), CmdFail> {
        self.commands.push(line.to_string());
        self.run(line.trim())
    }

    fn exit(&mut self, point: &str, req: &ExitRequest) -> ExitAnswer {
        let none = ExitAnswer {
            allow: true,
            outputs: req.parms(),
            called: Vec::new(),
            error: None,
        };
        match &mut self.exits {
            Some(f) => f(point, req).unwrap_or(none),
            None => none,
        }
    }

    fn file(&mut self, lib: &str, file: &str) -> Option<DbFile> {
        if !self.file_exists(lib, file) {
            return None;
        }
        self.files().get(&(lib.to_string(), file.to_string())).map(|(d, _)| d.clone())
    }

    fn sysval(&mut self, name: &str) -> Option<String> {
        (name == "QCCSID").then(|| "65535".into())
    }

    fn job_ccsid(&self) -> u16 {
        self.ccsid
    }

    fn now(&self) -> i64 {
        self.clock
    }

    fn utc_offset(&self) -> i64 {
        self.offset
    }
}
