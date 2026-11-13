// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `EXPORTFS` (`CHGNFSEXP`) in the service (docs/NFS.md section 6.1):
//! the flags, `/etc/exports` read and written as the job, each export
//! checked and put in the table, and IBM's messages: `CPCA1C4` an entry
//! exported, `CPCA1C6` the count for `-A`, `CPDA1C4` no entry in the
//! file, `CPDA1C5` an entry that failed with its reason, `CPFA1C7` the
//! escape when the command fails (with `-E`, at the first failure).

use ifs::client::IfsClient;
use ifscmd::{Msg, Reply};
use nfs::exports::{
    DataCcsid, EntryError, Export, HostOpt, PathCcsid, Request, file, flags, options,
};

use super::table::Entry;
use crate::ipl::system::{System, now_unix};

/// The file `EXPORTFS` reads.
pub const EXPORTS: &str = "/etc/exports";

/// File systems no directory of which can be exported.
const NOT_EXPORTABLE: &[&str] = &["QOPT", "QDLS", "QNTC", "QFileSvr.400", "dev"];

/// What the command asked, out of its parameters.
pub struct Args {
    /// `OPTIONS`.
    pub options: String,
    /// `DIR`, empty when not given.
    pub dir: String,
    /// `HOSTOPT`, `None` for `*DFT`.
    pub hosts: Option<Vec<HostOpt>>,
}

/// What the anonymous user resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anon {
    /// The UID; `None` refuses.
    pub uid: Option<u32>,
    /// Its profile, `*NONE` when none.
    pub profile: String,
}

/// What the work needs of the system, so it runs over a test double.
pub trait Host {
    /// The job's IFS.
    fn ifs(&mut self, f: &mut dyn FnMut(&dyn IfsClient) -> Reply) -> Reply;
    /// The job's CCSID.
    fn job_ccsid(&mut self) -> u16;
    /// The UID's profile, and `QNFSANON`'s UID.
    fn anon(&mut self, anon: Option<i64>) -> Anon;
    /// Put an entry in the table: its ID.
    fn put(&mut self, e: Entry) -> u32;
    /// Take the entry of a path out: whether there was one.
    fn remove(&mut self, path: &str) -> bool;
    /// The job, qualified.
    fn job(&mut self) -> String;
}

/// Run `EXPORTFS`.
pub fn run(h: &mut dyn Host, a: &Args) -> Reply {
    let flags = match flags::parse(&a.options) {
        Ok(f) => f,
        Err(e) => return Reply::fail(opt_msg(&e)),
    };
    let dir = (!a.dir.trim().is_empty()).then(|| a.dir.trim().to_string());
    let req = match flags.request(dir.as_deref()) {
        Ok(r) => r,
        Err(nfs::exports::FlagError::Opt(o)) => return Reply::fail(opt_msg(&o)),
        Err(e) => return Reply::fail(Msg::new(e.msgid())),
    };
    match req {
        Request::ExportAll | Request::UnexportAll => all(h, &req, flags.escape, flags.verbose),
        Request::Export {
            dir,
            ignore_file,
            options,
            update_file,
        } => {
            let mut out = Reply::default();
            let base = if ignore_file {
                None
            } else {
                match read_file(h) {
                    Ok(text) => {
                        let entries = file::parse(&text);
                        match file::find(&entries, &dir) {
                            Some(Ok(e)) => Some(e.clone()),
                            Some(Err(e)) => {
                                return fail_entry(out, &dir, e.reason);
                            }
                            None => {
                                out = out.diag(Msg::paths("CPDA1C4", &[&dir, EXPORTS]));
                                None
                            }
                        }
                    }
                    Err(_) => {
                        out = out.diag(Msg::paths("CPDA1C4", &[&dir, EXPORTS]));
                        None
                    }
                }
            };
            let mut e = base.unwrap_or_else(|| Export::new(&file::normal(&dir)));
            e.path = file::normal(&dir);
            if let Some(o) = options {
                e.options = o;
            }
            if let Some(hosts) = &a.hosts {
                e.hosts.clone_from(hosts);
            }
            match export_one(h, e) {
                Ok(e) => {
                    if update_file && let Err(r) = write_file(h, |t| file::upsert(t, &e)) {
                        return fail_entry(out.diag(r), &e.path, 23);
                    }
                    out.messages
                        .push((ifscmd::msg::COMPLETION, Msg::path("CPCA1C4", &e.path)));
                    out
                }
                Err((reason, m)) => {
                    if let Some(m) = m {
                        out = out.diag(m);
                    }
                    fail_entry(out, &dir, reason)
                }
            }
        }
        Request::Unexport { dir, update_file } => {
            let mut out = Reply::default();
            let path = file::normal(&dir);
            if !h.remove(&path) {
                return fail_entry(out, &path, 20);
            }
            if update_file && let Err(r) = write_file(h, |t| file::remove(t, &path)) {
                return fail_entry(out.diag(r), &path, 23);
            }
            out.messages
                .push((ifscmd::msg::COMPLETION, Msg::path("CPCA1C4", &path)));
            out
        }
    }
}

/// `-A` and `-A -U`: every entry of the file.
fn all(h: &mut dyn Host, req: &Request, escape: bool, verbose: bool) -> Reply {
    let mut out = Reply::default();
    let text = match read_file(h) {
        Ok(t) => t,
        Err(_) => return fail_entry(out, EXPORTS, 1),
    };
    let (mut done, mut failed) = (0i64, 0i64);
    for entry in file::parse(&text) {
        let r: Result<String, (String, u32, Option<Msg>)> = match entry {
            Err(EntryError { path, reason, .. }) => Err((path, reason, None)),
            Ok(e) if *req == Request::UnexportAll => {
                let p = e.path.clone();
                if h.remove(&p) {
                    Ok(p)
                } else {
                    Err((p, 20, None))
                }
            }
            Ok(e) => {
                let p = e.path.clone();
                export_one(h, e).map(|e| e.path).map_err(|(r, m)| (p, r, m))
            }
        };
        match r {
            Ok(p) => {
                done += 1;
                if verbose {
                    out.messages
                        .push((ifscmd::msg::INFO, Msg::path("CPCA1C4", &p)));
                }
            }
            Err((p, reason, m)) => {
                failed += 1;
                if let Some(m) = m {
                    out = out.diag(m);
                }
                let name = if p.is_empty() { EXPORTS } else { &p };
                if escape {
                    return fail_entry(out, name, reason);
                }
                out = out.diag(Msg::path("CPDA1C5", name).bin4(i64::from(reason)));
            }
        }
    }
    out.messages.push((
        ifscmd::msg::COMPLETION,
        Msg::new("CPCA1C6").bin4(done).bin4(failed),
    ));
    out
}

/// The entry failed: `CPDA1C5` with its reason, then the escape.
fn fail_entry(out: Reply, path: &str, reason: u32) -> Reply {
    let mut out = out.diag(Msg::path("CPDA1C5", path).bin4(i64::from(reason)));
    out.escape = Some(Msg::path("CPFA1C7", path).bin4(0));
    out
}

/// One export checked and put in the table; the reason code and a
/// message before it when it fails.
fn export_one(h: &mut dyn Host, mut e: Export) -> Result<Export, (u32, Option<Msg>)> {
    // The directory, as the job sees it.
    let path = e.path.clone();
    let mut problem: Option<(u32, Option<Msg>)> = None;
    h.ifs(&mut |c| {
        match c.stat(&path, true) {
            Ok(s) if s.mode & 0o170_000 == 0o040_000 => {}
            Ok(_) => problem = Some((5, Some(Msg::path("CPFA0B2", &path)))),
            Err(err) => problem = Some((4, Some(Msg::errno(err, &path)))),
        }
        if problem.is_none() {
            match c.statvfs(&path) {
                Ok(v)
                    if NOT_EXPORTABLE
                        .iter()
                        .any(|n| v.name.eq_ignore_ascii_case(n)) =>
                {
                    problem = Some((5, None));
                }
                // A remote mount (docs/NFS.md section 7) is host:path.
                Ok(v) if v.name.contains(':') => problem = Some((5, None)),
                Ok(_) => {}
                Err(err) => problem = Some((4, Some(Msg::errno(err, &path)))),
            }
        }
        Reply::default()
    });
    if let Some(p) = problem {
        return Err(p);
    }
    if let Some(r) = &e.options.nfsroot
        && let Some(x) = &e.options.exname
        && !x.starts_with(r.as_str())
    {
        return Err((21, None));
    }
    // The CCSIDs a host's *ASCII and *JOBCCSID stand for now.
    let job = h.job_ccsid();
    for o in &mut e.hosts {
        o.data = match o.data {
            DataCcsid::Ascii => DataCcsid::Ccsid(ascii_of(job)),
            DataCcsid::Job => DataCcsid::Ccsid(job),
            other => other,
        };
        o.path = match o.path {
            PathCcsid::Ascii => PathCcsid::Ccsid(ascii_of(job)),
            PathCcsid::Job => PathCcsid::Ccsid(job),
            other => other,
        };
        if let DataCcsid::Ccsid(n) = o.data
            && double_byte(n)
        {
            return Err((
                6,
                Some(Msg::new("CPDA1CA").bin4(i64::from(n)).chars("*SBCS", 9)),
            ));
        }
    }
    let anon = h.anon(e.options.anon);
    let entry = Entry {
        id: 0,
        export: e.clone(),
        anon_uid: anon.uid,
        anon_profile: anon.profile,
        by: h.job(),
        since: now_unix(),
    };
    h.put(entry);
    Ok(e)
}

/// Whether a CCSID holds double-byte characters (refused,
/// docs/NFS.md section 10).
fn double_byte(n: u16) -> bool {
    ccsid::conv::Codec::of(n)
        .ok()
        .flatten()
        .is_some_and(|c| c.is_mixed() || matches!(c.form, ccsid::conv::Form::Dbcs { .. }))
}

/// The ASCII CCSID matching an EBCDIC one (819 for the Latin-1 ones).
fn ascii_of(ccsid: u16) -> u16 {
    match ccsid {
        1208 | 819 | 850 | 437 | 1252 => ccsid,
        _ => 819,
    }
}

/// The message of an option list that is not valid.
pub fn opt_msg(e: &nfs::exports::OptError) -> Msg {
    use nfs::exports::OptError as E;
    match e {
        E::Unknown { pos } => Msg::new("CPFA1B0").bin4(i64::try_from(*pos).unwrap_or(0)),
        E::Duplicate { name } => Msg::new("CPFA1B1").vary(name),
        E::BadValue { name, value } => Msg::new("CPFA1BB").vary(value).vary(name),
    }
}

fn read_file(h: &mut dyn Host) -> Result<String, ()> {
    let mut text = None;
    h.ifs(&mut |c| {
        text = crate::cpp::stmf::read_text(c, EXPORTS).ok();
        Reply::default()
    });
    text.ok_or(())
}

fn write_file(h: &mut dyn Host, edit: impl Fn(&str) -> String) -> Result<(), Msg> {
    let mut err = None;
    h.ifs(&mut |c| {
        let old = crate::cpp::stmf::read_text(c, EXPORTS).unwrap_or_default();
        if let Err(e) = crate::cpp::stmf::write_text(c, EXPORTS, &edit(&old)) {
            err = Some(Msg::path("CPFA0A1", &e));
        }
        Reply::default()
    });
    err.map_or(Ok(()), Err)
}

/// `HOSTOPT` out of the parameter block: `None` for `*DFT`.
#[must_use]
pub fn host_opts(parms: &cmd::block::Parms) -> Option<Vec<HostOpt>> {
    let items = parms.list("HOSTOPT");
    if items.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for it in items {
        let cmd::Value::Elems(el) = it else {
            // The single value *DFT.
            return None;
        };
        let text = |i: usize| {
            el.get(i)
                .and_then(Option::as_ref)
                .map(cmd::Value::source)
                .unwrap_or_default()
        };
        let num = |i: usize| text(i).trim().parse::<i64>().ok();
        let host = text(0).trim().trim_matches('\'').to_string();
        let Some(spec) = options::host(&host) else {
            continue;
        };
        let mut o = HostOpt::new(spec);
        o.data = match num(1) {
            Some(-2) => DataCcsid::Ascii,
            Some(-3) => DataCcsid::Job,
            Some(n) if n >= 0 => DataCcsid::Ccsid(u16::try_from(n).unwrap_or(u16::MAX)),
            _ => DataCcsid::Binary,
        };
        o.path = match num(2) {
            Some(-2) => PathCcsid::Job,
            Some(n) if n >= 0 => PathCcsid::Ccsid(u16::try_from(n).unwrap_or(u16::MAX)),
            _ => PathCcsid::Ascii,
        };
        o.sync = num(3) != Some(-2);
        out.push(o);
    }
    Some(out)
}

/// The service as the host of the work.
pub struct Service<'a> {
    /// The system.
    pub system: &'a mut System,
}

impl Host for Service<'_> {
    fn ifs(&mut self, f: &mut dyn FnMut(&dyn IfsClient) -> Reply) -> Reply {
        crate::cpp::stmf::with_job(self.system, |c| f(c))
    }

    fn job_ccsid(&mut self) -> u16 {
        crate::stgsvc::ifs::job_ccsid(self.system, self.system.current_job)
    }

    fn anon(&mut self, anon: Option<i64>) -> Anon {
        super::anon(self.system, anon)
    }

    fn put(&mut self, e: Entry) -> u32 {
        super::state().table.put(e).0
    }

    fn remove(&mut self, path: &str) -> bool {
        super::state().table.remove(path)
    }

    fn job(&mut self) -> String {
        let n = self.system.current_job;
        self.system.jobs.get(n).map_or_else(
            || format!("{n:06}"),
            |j| format!("{:06}/{}/{}", j.number, j.user, j.name),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use ifs::backend::mem::MemBackend;
    use ifs::client::LocalIfs;
    use sysapi::ifs::memory::MemIfs;

    /// The work over an IFS in memory and a table of its own.
    struct Mem {
        _sys: MemIfs,
        ifs: LocalIfs<MemBackend>,
        table: super::super::table::ExportTable,
        anon: BTreeMap<i64, String>,
    }

    impl Mem {
        fn new(exports: Option<&str>) -> Self {
            let sys = MemIfs::new();
            let ifs = sys.client("QSECOFR", 1, 37);
            for d in ["/etc", "/home", "/home/joe", "/home/my data"] {
                let _ = ifs.mkdir(d, 0o755);
            }
            if let Some(t) = exports {
                crate::cpp::stmf::write_text(&ifs, EXPORTS, t).unwrap();
            }
            Self {
                _sys: sys,
                ifs,
                table: super::super::table::ExportTable::default(),
                anon: BTreeMap::new(),
            }
        }
    }

    impl Host for Mem {
        fn ifs(&mut self, f: &mut dyn FnMut(&dyn IfsClient) -> Reply) -> Reply {
            f(&self.ifs)
        }
        fn job_ccsid(&mut self) -> u16 {
            37
        }
        fn anon(&mut self, anon: Option<i64>) -> Anon {
            match anon {
                Some(-1) => Anon {
                    uid: None,
                    profile: "*NONE".into(),
                },
                a => Anon {
                    uid: Some(u32::try_from(a.unwrap_or(4)).unwrap_or(0)),
                    profile: self
                        .anon
                        .get(&a.unwrap_or(4))
                        .cloned()
                        .unwrap_or("QNFSANON".into()),
                },
            }
        }
        fn put(&mut self, e: Entry) -> u32 {
            self.table.put(e).0
        }
        fn remove(&mut self, path: &str) -> bool {
            self.table.remove(path)
        }
        fn job(&mut self) -> String {
            "000001/QSECOFR/QPADEV0001".into()
        }
    }

    fn args(options: &str, dir: &str) -> Args {
        Args {
            options: options.into(),
            dir: dir.into(),
            hosts: None,
        }
    }

    fn ids(r: &Reply) -> Vec<String> {
        r.messages
            .iter()
            .map(|(_, m)| m.id.clone())
            .chain(r.escape.iter().map(|m| format!("!{}", m.id)))
            .collect()
    }

    const FILE: &str = "/home/joe -ro,anon=-1\n\"/home/my data\" -rw=alpha HOSTOPT alpha:*ASCII:*JOBCCSID:*ASYNC\n/missing -ro\n/home -bogus\n";

    #[test]
    fn one_directory() {
        let mut m = Mem::new(Some(FILE));
        let r = run(&mut m, &args("*DFT", "/home/joe"));
        assert_eq!(ids(&r), ["CPCA1C4"]);
        let e = m.table.get("/home/joe").unwrap();
        assert!(e.export.options.ro);
        assert_eq!(e.anon_uid, None);
        // Not in the file: the defaults, and CPDA1C4 first.
        let r = run(&mut m, &args("-O 'nosuid'", "/etc"));
        assert_eq!(ids(&r), ["CPDA1C4", "CPCA1C4"]);
        assert!(m.table.get("/etc").unwrap().export.options.nosuid);
        // -I does not read the file.
        let r = run(&mut m, &args("-I", "/home/joe"));
        assert_eq!(ids(&r), ["CPCA1C4"]);
        assert!(!m.table.get("/home/joe").unwrap().export.options.ro);
        // The host options' CCSIDs are resolved.
        run(&mut m, &args("*DFT", "/home/my data"));
        let h = &m.table.get("/home/my data").unwrap().export.hosts[0];
        assert_eq!(
            (h.data, h.path, h.sync),
            (DataCcsid::Ccsid(819), PathCcsid::Ccsid(37), false)
        );
    }

    #[test]
    fn refusals() {
        let mut m = Mem::new(Some(FILE));
        assert_eq!(ids(&run(&mut m, &args("-Z", "/x"))), ["!CPFA1B0"]);
        assert_eq!(ids(&run(&mut m, &args("-A", "/x"))), ["!CPFA1B3"]);
        assert_eq!(ids(&run(&mut m, &args("-U", ""))), ["!CPFA1C4"]);
        assert_eq!(
            ids(&run(&mut m, &args("-I -O 'vers=2'", "/x"))),
            ["!CPFA1BB"]
        );
        // A directory that is not there.
        let r = run(&mut m, &args("-I", "/nope"));
        assert_eq!(r.escape.as_ref().unwrap().id, "CPFA1C7");
        assert!(ids(&r).contains(&"CPDA1C5".to_string()));
        // An entry of the file that is not written correctly.
        let r = run(&mut m, &args("*DFT", "/home"));
        assert_eq!(ids(&r), ["CPDA1C5", "!CPFA1C7"]);
        // Unexporting what is not exported.
        assert_eq!(
            ids(&run(&mut m, &args("-U", "/etc"))),
            ["CPDA1C5", "!CPFA1C7"]
        );
    }

    #[test]
    fn every_entry() {
        let mut m = Mem::new(Some(FILE));
        let r = run(&mut m, &args("-A", ""));
        // /missing and /home fail; the count says 2 and 2.
        assert_eq!(ids(&r), ["CPFA0A9", "CPDA1C5", "CPDA1C5", "CPCA1C6"]);
        let last = &r.messages.last().unwrap().1;
        assert_eq!(last.data, [0, 0, 0, 2, 0, 0, 0, 2]);
        assert_eq!(m.table.entries().len(), 2);
        // -E stops at the first failure.
        let mut m = Mem::new(Some(FILE));
        let r = run(&mut m, &args("-A -E", ""));
        assert_eq!(r.escape.as_ref().unwrap().id, "CPFA1C7");
        // -V names every entry.
        let mut m = Mem::new(Some("/etc\n/home/joe\n"));
        let r = run(&mut m, &args("-A -V", ""));
        assert_eq!(ids(&r), ["CPCA1C4", "CPCA1C4", "CPCA1C6"]);
        let r = run(&mut m, &args("-A -U", ""));
        assert_eq!(ids(&r), ["CPCA1C6"]);
        assert!(m.table.entries().is_empty());
        // No file.
        let mut m = Mem::new(None);
        let r = run(&mut m, &args("-A", ""));
        assert_eq!(ids(&r), ["CPDA1C5", "!CPFA1C7"]);
    }

    #[test]
    fn the_file_is_kept_with_f() {
        let mut m = Mem::new(Some(FILE));
        let r = run(&mut m, &args("-I -F -O 'ro,root=alpha'", "/etc"));
        assert_eq!(ids(&r), ["CPCA1C4"]);
        let t = crate::cpp::stmf::read_text(&m.ifs, EXPORTS).unwrap();
        assert!(t.contains("/etc -ro,root=alpha\n"), "{t}");
        let r = run(&mut m, &args("-U -I -F", "/etc"));
        assert_eq!(ids(&r), ["CPCA1C4"]);
        let t = crate::cpp::stmf::read_text(&m.ifs, EXPORTS).unwrap();
        assert!(!t.contains("/etc"));
        assert!(t.starts_with("/home/joe -ro,anon=-1\n"));
    }
}
