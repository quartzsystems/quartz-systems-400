// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate stores in the service: the `*SYSTEM` store's stash
//! opens with the LIC partition's key, which only the service holds, so
//! the `QS*` commands, a server job's certificate (`cert_app`), the
//! certificate displays and the daily expiry scan all run here. The
//! store files live in the IFS under `/QIBM/UserData/ICSS/Cert`, read
//! and written as the system: a request stages the files it may touch
//! in memory, `certs::admin` works on them, and what changed goes back
//! (each file replaced through a rename, so a reader never sees half).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock, PoisonError};

use certs::admin::{self, Ctx, Keystore, expiry};
use certs::crypto::kdf::Argon2Params;
use certs::files::{MemFiles, StoreFiles};
use certs::paths::{APP_DEFINITIONS, EXPIRY_STATE, LOCAL_CA_CERT, LOCAL_CA_STORE, SYSTEM_STORE, stash_path};
use certs::stash::KeySealer;
use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use ifs::ops::{OpenArgs, RenameMode};
use sec::auth::principal::Principal;
use sec::profile::SpecialAuth;
use stg::proto::{CertTrust, Sys2Data};

use super::ifs as service;
use crate::ipl::system::{System, now_unix};

/// The profiles the system's servers run as: they may have their
/// application's certificate.
const SERVER_PROFILES: &[&str] = &["QSYS", "QTCP", "QUSER", "QTMHHTTP", "QTMHHTP1", "QSPLJOB"];

/// How long a `cert_app` answer is reused while the files are unchanged.
const CACHE_SECS: u64 = 600;

/// The system as the store files' owner.
fn system_principal() -> Principal {
    Principal {
        user: "QSYS".into(),
        spcaut: SpecialAuth::ALL.0,
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    }
}

/// Run `f` against the IFS as the system.
fn as_system<T>(system: &mut System, f: impl FnOnce(&dyn IfsClient) -> T) -> T {
    let p = system_principal();
    service::with_client(system, 0, service::console_owner(0), &p, 37, f)
}

fn errno_text(path: &str, e: Errno) -> String {
    format!("{path}: {}", e.text())
}

/// A file's bytes; `None` when it does not exist.
fn read_file(c: &dyn IfsClient, path: &str) -> Result<Option<Vec<u8>>, String> {
    let fd = match c.open(path, OpenArgs::new(O_RDONLY)) {
        Ok(fd) => fd,
        Err(Errno::ENOENT | Errno::ENOTDIR) => return Ok(None),
        Err(e) => return Err(errno_text(path, e)),
    };
    let data = read_all(c, fd);
    let _ = c.close(fd);
    data.map(Some).map_err(|e| errno_text(path, e))
}

/// Make `dir` and its parents; owner-only for the new ones.
fn make_dirs(c: &dyn IfsClient, dir: &str) -> Result<(), String> {
    let mut at = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        at.push('/');
        at.push_str(part);
        match c.mkdir(&at, 0o700) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(e) => return Err(errno_text(&at, e)),
        }
    }
    Ok(())
}

/// Replace a file: written beside it, then renamed over it.
fn write_file(c: &dyn IfsClient, path: &str, data: &[u8]) -> Result<(), String> {
    if let Some(i) = path.rfind('/')
        && i > 0
    {
        make_dirs(c, &path[..i])?;
    }
    let tmp = format!("{path}.new");
    let fd = c
        .open(&tmp, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o600))
        .map_err(|e| errno_text(&tmp, e))?;
    let w = write_all(c, fd, data).and_then(|()| c.fsync(fd, false));
    let _ = c.close(fd);
    w.map_err(|e| errno_text(&tmp, e))?;
    c.rename(&tmp, path, RenameMode::Unlink)
        .map_err(|e| errno_text(path, e))
}

fn remove_file(c: &dyn IfsClient, path: &str) -> Result<(), String> {
    match c.unlink(path) {
        Ok(()) | Err(Errno::ENOENT) => Ok(()),
        Err(e) => Err(errno_text(path, e)),
    }
}

/// The store files a request may touch, staged in memory.
struct Staged {
    files: MemFiles,
    before: BTreeMap<String, Vec<u8>>,
}

/// The files of the system's stores, plus those of `extra` stores.
fn paths(extra: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = [SYSTEM_STORE, LOCAL_CA_STORE]
        .iter()
        .chain(extra)
        .flat_map(|p| [(*p).to_string(), stash_path(p)])
        .collect();
    v.extend([LOCAL_CA_CERT, APP_DEFINITIONS, EXPIRY_STATE].map(str::to_string));
    v.sort();
    v.dedup();
    v
}

fn stage(system: &mut System, extra: &[&str]) -> Result<Staged, String> {
    if let Some(why) = service::why_not() {
        return Err(format!("{} the integrated file system is not available: {why}", admin::msgs::FAILED));
    }
    let want = paths(extra);
    let files = MemFiles::new();
    let mut before = BTreeMap::new();
    as_system(system, |c| {
        for p in &want {
            if let Some(b) = read_file(c, p)? {
                files.write(p, &b).map_err(|e| e.to_string())?;
                before.insert(p.clone(), b);
            }
        }
        Ok::<(), String>(())
    })?;
    Ok(Staged { files, before })
}

/// Write back what changed; remove what went.
fn commit(system: &mut System, s: &Staged) -> Result<(), String> {
    let now: BTreeMap<String, Vec<u8>> = s
        .files
        .paths()
        .into_iter()
        .filter_map(|p| s.files.read(&p).ok().flatten().map(|b| (p, b)))
        .collect();
    as_system(system, |c| {
        for (p, b) in &now {
            if s.before.get(p) != Some(b) {
                write_file(c, p, b)?;
            }
        }
        for p in s.before.keys() {
            if !now.contains_key(p) {
                remove_file(c, p)?;
            }
        }
        Ok(())
    })
}

/// The stash key from the LIC partition's record, read fresh.
fn sealer(system: &mut System) -> Option<KeySealer> {
    let (part, record) = system.lic.as_mut()?;
    part.refresh(record);
    KeySealer::from_record(record)
}

/// Run `f` over the staged files of the system's stores and `extra`;
/// with `write` what it changed is written back (also when it failed:
/// a command may have done part of its work).
pub fn with_stores<T>(
    system: &mut System,
    extra: &[&str],
    write: bool,
    f: impl FnOnce(&Ctx<'_>) -> T,
) -> Result<T, String> {
    let staged = stage(system, extra)?;
    let key = sealer(system);
    let ctx = Ctx {
        files: &staged.files,
        sealer: key.as_ref().map(|k| k as &dyn certs::stash::Sealer),
        now: now_unix(),
        params: Argon2Params::default(),
    };
    let out = f(&ctx);
    if write {
        commit(system, &staged)?;
    }
    Ok(out)
}

/// Whether `p` (the request's actor; `None` is the system) may manage
/// certificates: `*ALLOBJ` and `*SECADM`, as DCM asks. A refusal is
/// audited.
pub fn may_manage(p: Option<&Principal>) -> Result<(), String> {
    let Some(p) = p else {
        return Ok(());
    };
    if p.has(SpecialAuth::ALLOBJ) && p.has(SpecialAuth::SECADM) {
        return Ok(());
    }
    refused_audit(&p.user, "manage certificates");
    Err(p.user.clone())
}

fn refused_audit(user: &str, what: &str) {
    sec::audit::record(
        sec::audit::Event::new(sec::audit::Kind::Af)
            .set("VIOL", "A")
            .object("DEFAULT", "", "*STMF")
            .set("OPER", what)
            .by(user),
    );
}

/// A job may have an application's certificate: a system server
/// profile, or a user with `*ALLOBJ` and `*IOSYSCFG`.
fn may_have_key(p: &Principal) -> bool {
    SERVER_PROFILES.contains(&p.user.as_str())
        || (p.has(SpecialAuth::ALLOBJ) && p.has(SpecialAuth::IOSYSCFG))
}

/// Answers by application: the files' fingerprint, when, the answer.
type Cache = Mutex<HashMap<String, (u64, u64, CertTrust)>>;

/// Answers kept while the store files are unchanged.
fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fingerprint(files: &MemFiles) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in files.paths() {
        p.hash(&mut h);
        files.read(&p).ok().flatten().hash(&mut h);
    }
    h.finish()
}

/// `cert_app`.
pub fn cert_app(system: &mut System, principal: &Principal, app_id: &str) -> Result<CertTrust, String> {
    let id = app_id.trim();
    if !id.is_empty() && !may_have_key(principal) {
        refused_audit(&principal.user, "certificate key");
        return Err(format!(
            "{} {} may not have the certificate of {id}",
            admin::msgs::NOT_AUTHORIZED,
            principal.user
        ));
    }
    let staged = stage(system, &[])?;
    let print = fingerprint(&staged.files);
    let now = now_unix();
    if let Some((p, at, t)) = cache().lock().unwrap_or_else(PoisonError::into_inner).get(id)
        && *p == print
        && now.saturating_sub(*at) < CACHE_SECS
    {
        return Ok(t.clone());
    }
    let key = sealer(system);
    let ctx = Ctx {
        files: &staged.files,
        sealer: key.as_ref().map(|k| k as &dyn certs::stash::Sealer),
        now,
        params: Argon2Params::default(),
    };
    let t = admin::identity::for_app(&ctx, id).map_err(|e| {
        let n = admin::refusal(&e, "*SYSTEM");
        format!("{} {e}", n.msgid)
    })?;
    let reply = to_wire(t);
    cache()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(id.to_string(), (print, now, reply.clone()));
    Ok(reply)
}

fn to_wire(t: admin::identity::AppTrust) -> CertTrust {
    let mut out = CertTrust {
        roots: t.roots,
        ..CertTrust::default()
    };
    if let Some(i) = t.identity {
        out.label = i.label;
        out.chain = i.chain;
        out.key = i.key.to_vec();
        out.client_auth = match i.client_auth {
            certs::apps::ClientAuth::None => 0,
            certs::apps::ClientAuth::Optional => 1,
            certs::apps::ClientAuth::Required => 2,
        };
    }
    out
}

/// `cert_expiry`, and `QSCHKCRT`: scan, send the warnings to `QSYSOPR`;
/// a line for each warning and store skipped.
pub fn expiry_scan(system: &mut System) -> Vec<String> {
    let r = with_stores(system, &[], true, |ctx| expiry::scan(ctx, &expiry::watched()));
    let scan = match r {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return vec![format!("{} {e}", admin::msgs::FAILED)],
        Err(e) => return vec![e],
    };
    let mut lines = Vec::new();
    for w in &scan.warnings {
        let n = w.note();
        let data = super::super::cpp::certcmd::data(n.msgid, &n.data);
        let text = super::super::cpp::certcmd::text(n.msgid, &data);
        let m = wm::obj::msgq::QMsg {
            msgid: n.msgid.into(),
            msgf: ("QTCPMSG".into(), "QSYS".into()),
            severity: if w.threshold == 0 { 40 } else { 30 },
            text: text.clone(),
            data,
            job: ("QSYSCOMM1".into(), "QSYS".into(), 0),
            from_pgm: "QSZCCHK".into(),
            ..wm::obj::msgq::QMsg::default()
        };
        let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
        lines.push(format!("{} {text}", n.msgid));
    }
    for (store, why) in &scan.skipped {
        lines.push(format!("{} {store}: {why}", admin::msgs::NO_STASH));
    }
    lines
}

/// `cert_panel`: the data of `QSWRKCRT`, its display, and `QSWRKAPPID`.
pub fn panel(system: &mut System, principal: &Principal, what: &str, args: &[String]) -> Result<Sys2Data, String> {
    let arg = |i: usize| args.get(i).map_or("", String::as_str);
    match what {
        "CRT" | "DSPCRT" => {
            may_manage(Some(principal)).map_err(|u| format!("{} {u}", admin::msgs::NOT_AUTHORIZED))?;
            let ks = Keystore::new(if arg(0).is_empty() { "*SYSTEM" } else { arg(0) }, arg(1));
            let path = ks.path.clone();
            let rows = with_stores(system, &[path.as_str()], false, |ctx| admin::entries::list(ctx, &ks))?
                .map_err(|e| {
                    let n = admin::refusal(&e, ks.shown());
                    format!("{} {e}", n.msgid)
                })?;
            if what == "CRT" {
                Ok(super::super::cpp::certcmd::crt_rows(ks.shown(), &rows))
            } else {
                let r = rows
                    .iter()
                    .find(|r| r.label == arg(2))
                    .ok_or_else(|| format!("{} no certificate has label {}", admin::msgs::NO_LABEL, arg(2)))?;
                Ok(super::super::cpp::certcmd::crt_detail(ks.shown(), r))
            }
        }
        "APPID" => {
            may_manage(Some(principal)).map_err(|u| format!("{} {u}", admin::msgs::NOT_AUTHORIZED))?;
            let apps = with_stores(system, &[], false, admin::appids::list)?
                .map_err(|e| format!("{} {e}", admin::msgs::FAILED))?;
            Ok(super::super::cpp::certcmd::app_rows(&apps))
        }
        other => Err(format!("CPF9898 no certificate display {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_the_system_files_and_the_named_store() {
        let p = paths(&["/home/a/b.kdb"]);
        for want in [SYSTEM_STORE, "/QIBM/UserData/ICSS/Cert/Server/DEFAULT.RDB", "/home/a/b.RDB", APP_DEFINITIONS, EXPIRY_STATE] {
            assert!(p.iter().any(|x| x == want), "{want}");
        }
        assert_eq!(paths(&[SYSTEM_STORE]).len(), paths(&[]).len());
    }

    #[test]
    fn servers_may_have_keys() {
        let p = |user: &str, spcaut: u32| Principal {
            user: user.into(),
            spcaut,
            ..Principal::default()
        };
        assert!(may_have_key(&p("QTCP", 0)));
        assert!(may_have_key(&p("QTMHHTTP", 0)));
        assert!(!may_have_key(&p("JOE", SpecialAuth::ALLOBJ.0)));
        assert!(may_have_key(&p("ADMIN", SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0)));
    }
}
