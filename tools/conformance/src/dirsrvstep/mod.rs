// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's steps of the disk suite (`docs/DIRSRV.md`
//! sections 16 and 18), on the main guest after step 107 (whose
//! `QSSECCHK` wants the shipped instance):
//!
//! | Step | What |
//! |---|---|
//! | 193 | `QSSECCHK`: the shipped `QUSRDIR` gives the three `DIRSRV` warnings (first, before 186 sets a password) |
//! | 186 | `CHGDIRSVRA DN(*ADMINDN ..)`, `STRTCPSVR SERVER(*DIRSRV)`: `QUSRDIR` as `QDIRSRV` in `QSYSWRK` on 389, `GLD0269`; `ENDTCPSVR`, `GLD0265`; started again |
//! | 187 | `tools/ldaptest` from the host (read, controls, ext, write, acl, ldif); its read-only answers diffed with the primary's record |
//! | 188 | the projected backend: profile binds (`ldaptest` projected), `QMAXSIGN` and `QMAXSGNACN`, a `PW` entry, `QIBM_DIRSRV_ADMIN` |
//! | 189 | TLS: a certificate assigned to `QS400_DIRECTORY_SERVER`, `ibm-slapdSecurity: SSLTLS`, LDAPS on 636 and `StartTLS` on 389, SASL `EXTERNAL` with a client certificate |
//! | 190 | in the guest: the QSH `ldap*` tools and an ILE program over `QGLDCLNT` |
//! | 191 | publishing: the users agent set through Navigator, a `CRTUSRPRF` appears and a `DLTUSRPRF` disappears |
//! | 192 | persistence across `ENDTCPSVR`/`STRTCPSVR` and an IPL, `SAVLIB`/`RSTLIB` of `QUSRDIRDB`, `RCLSTG` |
//! | 194 | `ldaptest bench`: binds a minute and a paged search (numbers to `DIRSRV.md` §16) |
//!
//! The harness's own setup and checks over the forwarded ports use
//! `net/ldap`'s client; what is under test from the outside is
//! `ldaptest`, which shares no code with the server.

mod prog;
mod steps;
mod tool;

use std::time::{Duration, Instant};

use ldap::client::{Client, Options, SearchResult};
use ldap::msg::{Attribute, ModOp, Modification, Scope};

use crate::qemu::session::Session;

/// The administrator.
pub(crate) const ADMIN_DN: &str = "cn=Administrator";
/// Its password, set by step 186 (not a well-known one: `QSSECCHK` would
/// call it a failure).
pub(crate) const ADMIN_PW: &str = "Dir2026qs";
/// The instance.
pub(crate) const INSTANCE: &str = "QUSRDIR";
/// The suffix the writing scenarios work under (the shipped instance has
/// no user suffix; `cn=localhost` is an entry store's).
pub(crate) const SUFFIX: &str = "cn=localhost";
/// The command time-out of the console.
pub(crate) const CMD: Duration = crate::steps::CMD;
/// A command that starts or ends jobs.
pub(crate) const SLOW: Duration = Duration::from_secs(120);

/// What the steps set up, for the next ones.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// `CHGDIRSVRA` set the administrator's password.
    pub admin_set: bool,
    /// The instance runs.
    pub started: bool,
    /// The host ports forwarded to 389 and 636.
    pub ports: Option<(u16, u16)>,
}

/// Escape (`CPF`) or diagnostic (`CPD`) messages, or `GLD` ones of
/// severity 40 the command sent, in a command's output.
pub(crate) fn errors(out: &str) -> Vec<&str> {
    let mut v = crate::nfsstep::errors(out);
    v.extend(out.lines().filter(|l| {
        let w = l.split_whitespace().next().unwrap_or("");
        w.len() == 7
            && w.starts_with("GLD")
            && !matches!(w, "GLD0123" | "GLD0269" | "GLD0265" | "GLD010B")
    }));
    v
}

/// A command that must end without an escape or diagnostic message.
pub(crate) fn clean(s: &mut Session, line: &str, limit: Duration) -> Result<String, String> {
    let out = s.command(line, limit)?;
    let bad = errors(&out);
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(format!("{line}: {}\n{out}", bad.join("; ")))
    }
}

/// A command that must send one of `ids` (in its output or the job log).
pub(crate) fn want(
    s: &mut Session,
    line: &str,
    ids: &[&str],
    limit: Duration,
) -> Result<String, String> {
    crate::nfsstep::want(s, line, ids, limit)
}

/// `QSH CMD(script)`: its output and whether it ended with status 0.
pub(crate) fn qsh(s: &mut Session, script: &str, limit: Duration) -> Result<(bool, String), String> {
    debug_assert!(!script.contains('\''));
    let out = s.command(&format!("QSH CMD('{script}')"), limit)?;
    Ok((out.contains("exit status 0."), out))
}

/// The LDAP client over a forwarded port, with a time-out.
pub(crate) fn connect(port: u16) -> Result<Client, String> {
    let opts = Options {
        timeout: Some(Duration::from_secs(60)),
        connect_timeout: Duration::from_secs(10),
        ..Options::default()
    };
    Client::connect("127.0.0.1", port, opts).map_err(|e| format!("connect to {port}: {e}"))
}

/// A session bound as `dn`; the bind's result code.
pub(crate) fn bind_rc(port: u16, dn: &str, pw: &str) -> Result<u32, String> {
    let mut c = connect(port)?;
    let r = c
        .simple_bind(dn, pw.as_bytes())
        .map_err(|e| format!("bind {dn}: {e}"))?;
    let _ = c.unbind();
    Ok(r.result.code.number())
}

/// A session bound as `dn`, which must succeed.
pub(crate) fn bound(port: u16, dn: &str, pw: &str) -> Result<Client, String> {
    let mut c = connect(port)?;
    let r = c
        .simple_bind(dn, pw.as_bytes())
        .map_err(|e| format!("bind {dn}: {e}"))?;
    if !r.result.is_success() {
        return Err(format!("bind {dn}: {:?}", r.result));
    }
    Ok(c)
}

/// A session bound as the administrator.
pub(crate) fn admin(port: u16) -> Result<Client, String> {
    bound(port, ADMIN_DN, ADMIN_PW)
}

/// A search; its result whatever the code.
pub(crate) fn search(
    c: &mut Client,
    base: &str,
    scope: Scope,
    filter: &str,
    attrs: &[&str],
) -> Result<SearchResult, String> {
    let req = c
        .search_request(base, scope, filter, attrs)
        .map_err(|e| e.to_string())?;
    c.search(&req, &[])
        .map_err(|e| format!("search {base} {filter}: {e}"))
}

/// An attribute of string values.
pub(crate) fn attr(ty: &str, vals: &[&str]) -> Attribute {
    Attribute {
        ty: ty.to_string(),
        vals: vals.iter().map(|v| v.as_bytes().to_vec()).collect(),
    }
}

/// Adds an entry; its result code.
pub(crate) fn add(c: &mut Client, dn: &str, attrs: &[(&str, &[&str])]) -> Result<u32, String> {
    let a = attrs.iter().map(|(t, v)| attr(t, v)).collect();
    let r = c.add(dn, a, &[]).map_err(|e| format!("add {dn}: {e}"))?;
    Ok(r.result.code.number())
}

/// Replaces an attribute's values; the result code.
pub(crate) fn replace(c: &mut Client, dn: &str, ty: &str, vals: &[&str]) -> Result<u32, String> {
    let m = Modification {
        op: ModOp::Replace,
        attr: attr(ty, vals),
    };
    let r = c
        .modify(dn, vec![m], &[])
        .map_err(|e| format!("modify {dn}: {e}"))?;
    Ok(r.result.code.number())
}

/// Deletes an entry; the result code.
pub(crate) fn delete(c: &mut Client, dn: &str) -> Result<u32, String> {
    let r = c.delete(dn, &[]).map_err(|e| format!("delete {dn}: {e}"))?;
    Ok(r.result.code.number())
}

/// Whether the entry `dn` is there (a base search as `c`).
pub(crate) fn exists(c: &mut Client, dn: &str) -> Result<bool, String> {
    let r = search(c, dn, Scope::Base, "(objectClass=*)", &["1.1"])?;
    Ok(r.result.is_success() && !r.entries.is_empty())
}

/// Wait until the server on host port `port` answers an anonymous bind.
pub(crate) fn wait_ldap(port: u16, limit: Duration) -> Result<(), String> {
    let end = Instant::now() + limit;
    loop {
        match bind_rc(port, "", "") {
            Ok(0) => return Ok(()),
            Ok(rc) if Instant::now() > end => return Err(format!("anonymous bind: {rc}")),
            Err(e) if Instant::now() > end => return Err(e),
            _ => std::thread::sleep(Duration::from_millis(500)),
        }
    }
}

/// The root DSE's first value of `attr`.
pub(crate) fn root_dse(port: u16, attr: &str) -> Result<String, String> {
    let mut c = connect(port)?;
    let r = search(&mut c, "", Scope::Base, "(objectClass=*)", &[attr])?;
    let _ = c.unbind();
    r.entries
        .first()
        .and_then(|e| e.first(attr))
        .map(str::to_string)
        .ok_or_else(|| format!("the root DSE has no {attr}: {r:?}"))
}

/// The `WRKACTJOB` line of `name`, waiting for it.
pub(crate) fn job_line(s: &mut Session, name: &str) -> Result<String, String> {
    crate::netsvrstep::job_line(s, name)
}

/// Waits for `name` to leave `WRKACTJOB`.
pub(crate) fn job_gone(s: &mut Session, name: &str) -> Result<(), String> {
    crate::netsvrstep::job_gone(s, name)
}

/// `STRTCPSVR SERVER(*DIRSRV)`: started (`TCP1A07`) or running already
/// (`TCP1A08`), the job active.
pub(crate) fn start(s: &mut Session) -> Result<String, String> {
    let out = want(s, "STRTCPSVR SERVER(*DIRSRV)", &["TCP1A07", "TCP1A08"], SLOW)?;
    job_line(s, INSTANCE)?;
    Ok(out)
}

/// `ENDTCPSVR SERVER(*DIRSRV)`, the job gone.
pub(crate) fn end(s: &mut Session) -> Result<String, String> {
    let out = want(s, "ENDTCPSVR SERVER(*DIRSRV)", &["TCP1A0A", "TCP1A09"], SLOW)?;
    job_gone(s, INSTANCE)?;
    Ok(out)
}

/// The console job keeps every message.
pub(crate) fn full_log(s: &mut Session) -> Result<(), String> {
    crate::nfsstep::full_log(s)
}

/// What the guest shows when a step fails: the instance's job log,
/// `QSYSOPR`, the jobs, the listeners and the serial log's end, in
/// `dirsrv-<step>.txt`; its path.
pub(crate) fn dump(dir: &std::path::Path, step: &str, s: &mut Session, extra: &str) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    if !extra.is_empty() {
        let _ = write!(text, "=== the host\n{extra}\n");
    }
    for c in [
        "svc.dspjoblog",
        "svc.dspjoblog JOB(QUSRDIR)",
        "svc.dspjoblog JOB(QGLDPUBE)",
        "DSPMSG MSGQ(QSYSOPR)",
        "wrkactjob",
        "svc.sql SELECT * FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT IN (389, 636);",
        "svc.ifs ls /QIBM/UserData/OS400/DirSrv",
    ] {
        let _ = writeln!(text, "=== {c}");
        match s.command(c, Duration::from_secs(30)) {
            Ok(out) => {
                let _ = writeln!(text, "{out}");
            }
            Err(e) => {
                let _ = writeln!(text, "(no answer: {e})");
                break;
            }
        }
    }
    let _ = std::fs::write(dir.join(format!("dirsrv-{step}.serial.txt")), &s.log);
    let path = dir.join(format!("dirsrv-{step}.txt"));
    match std::fs::write(&path, text) {
        Ok(()) => path.display().to_string(),
        Err(e) => format!("{} not written: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gld_errors() {
        let out = "GLD0123 changed\nGLD0202 no password\nCPF9898 x\nTCP1A07 started";
        assert_eq!(errors(out), ["CPF9898 x", "GLD0202 no password"]);
    }
}
