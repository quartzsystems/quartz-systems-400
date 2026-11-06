// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Function usage: the functions a profile may be allowed or denied
//! apart from object authority (`WRKFCNUSG`, `CHGFCNUSG`, `DSPFCNUSG`).
//! The functions are IBM's, as the primary oracle registers them at
//! V7R1M0 (`tools/conformance/oracle/rel/README.md`), with Quartz
//! names; what an administrator changes (a function's default, its use
//! of `*ALLOBJ`, each profile's usage) is kept in `QUSRSYS/QSYFCNUSG`
//! ([`crate::textspace`]), lines `F ID ALLOWED USED` and
//! `U ID PROFILE ALLOWED` after a `QFCNUSG 1` header.
//!
//! A profile may use a function when its own usage allows it; else when
//! one of its groups' allows it (and none denies it before); else when
//! the function uses `*ALLOBJ` and the profile has it; else when the
//! default allows it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use store::obj::ObjectId;
use store::txn::Txn;

use crate::auth::principal::Principal;
use crate::error::{Result, SecError};
use crate::profile::SpecialAuth;

/// The record's space and header.
const SPACE: (&str, &str) = ("QSYFCNUSG", "Function usage");
const HEADER: &str = "QFCNUSG 1";

/// One administrable function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// The function ID (`QIBM_DB_SQLADM`).
    pub id: &'static str,
    /// Its name, as `WRKFCNUSG` lists it.
    pub name: &'static str,
    /// What it controls.
    pub desc: &'static str,
    /// The product it belongs to.
    pub product: &'static str,
    /// Its group, `*NONE` for none.
    pub group: &'static str,
    /// Shipped default: allowed.
    pub default_allowed: bool,
    /// Shipped: `*ALLOBJ` allows it.
    pub allobj_used: bool,
}

const OS: &str = "QIBM_BASE_OPERATING_SYSTEM";
const TCP: &str = "QIBM_QTM_TCPIP";
const NAV: &str = "QIBM_QINAV_NAVIGATOR_WEB";

const fn f(
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    (product, group): (&'static str, &'static str),
    (default_allowed, allobj_used): (bool, bool),
) -> Function {
    Function {
        id,
        name,
        desc,
        product,
        group,
        default_allowed,
        allobj_used,
    }
}

/// Denied by default, `*ALLOBJ` does not count.
const DN: (bool, bool) = (false, false);
/// Denied by default, `*ALLOBJ` allows it.
const DU: (bool, bool) = (false, true);
/// Allowed by default.
const AN: (bool, bool) = (true, false);

/// The functions, in the order `WRKFCNUSG` lists them.
pub static FUNCTIONS: &[Function] = &[
    f("QIBM_DIRSRV_ADMIN", "Directory server administrator", "Bind as a directory server administrator", (OS, "*NONE"), DN),
    f("QIBM_ACCESS_ALLOBJ_JOBLOG", "Job log of an *ALLOBJ job", "A *JOBCTL user may see the job log of a job with *ALLOBJ special authority", (OS, "QIBM_ALLOBJ"), DU),
    f("QIBM_ALLOBJ_TRACE_ANY_USER", "Trace any user", "Trace jobs of any user", (OS, "QIBM_ALLOBJ"), DU),
    f("QIBM_WATCH_ANY_JOB", "Watch any job", "Start watches over jobs of any user", (OS, "QIBM_ALLOBJ"), DN),
    f("QIBM_DB_SQLADM", "Database administrator", "Administer the database: plan cache, monitors, statement holds", (OS, "QIBM_DB"), DN),
    f("QIBM_DB_SYSMON", "Database information", "See database information: plan cache and monitor data", (OS, "QIBM_DB"), DN),
    f("QIBM_QCST_SERVICE_CLUSTADMIN", "Cluster administration", "Administer a cluster", (OS, "QIBM_QCST_SERVICE_CLUSTMGMT"), DU),
    f("QIBM_QCST_SERVICE_CLUSTOPER", "Cluster operation", "Operate a cluster", (OS, "QIBM_QCST_SERVICE_CLUSTMGMT"), DU),
    f("QIBM_QYAS_SERVICE_DISKMGMT", "Disk units", "Work with disk units", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_SERVICE_DISK_WATCHER", "Disk watcher", "The disk watcher performance tool", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_SERVICE_DUMP", "Service dump", "Take service dumps", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_SERVICE_JOB_WATCHER", "Job watcher", "The job watcher performance tool", (OS, "QIBM_SERVICE"), DU),
    f("QIBM_SERVICE_THREAD", "Thread control", "Control threads", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_SERVICE_TRACE", "Service trace", "Start and end service traces", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_SERVICE_WATCH", "Service watch", "Start and end service watches", (OS, "QIBM_SERVICE"), DN),
    f("QIBM_QINAV_WEB_CONFIGURE", "Configure Navigator web", "Configure the web Navigator of this system", (NAV, "*NONE"), DU),
    f("QIBM_QINAV_WEB_FUNCTIONS", "Manage through Navigator web", "Manage this system through any web Navigator", (NAV, "*NONE"), DU),
    f("QIBM_QINAV_WEB_INTERFACE", "Use Navigator web", "Use the web Navigator of this system", (NAV, "*NONE"), DU),
    f("QIBM_QSY_SYSTEM_CERT_STORE", "*SYSTEM certificate store", "Use the *SYSTEM certificate store", ("QIBM_QSY_DIGITAL_CERT_MGR", "*NONE"), DU),
    f("QIBM_QTMF_CLIENT_REQ_0", "FTP client session", "Start an FTP client session; needed for every other client request", (TCP, "QIBM_QTMF_FTP_CLIENT"), AN),
    f("QIBM_QTMF_CLIENT_REQ_10", "FTP client CCC", "The client subcommand CCC", (TCP, "QIBM_QTMF_FTP_CLIENT_REQ"), AN),
    f("QIBM_QTMF_CLIENT_REQ_3", "FTP client LCD", "The client subcommand LCD", (TCP, "QIBM_QTMF_FTP_CLIENT_REQ"), AN),
    f("QIBM_QTMF_CLIENT_REQ_6", "FTP client sends", "The client subcommands PUT, MPUT and APPEND", (TCP, "QIBM_QTMF_FTP_CLIENT_REQ"), AN),
    f("QIBM_QTMF_CLIENT_REQ_7", "FTP client receives", "The client subcommands GET and MGET", (TCP, "QIBM_QTMF_FTP_CLIENT_REQ"), AN),
    f("QIBM_QTMF_CLIENT_REQ_9", "FTP client SYSCMD", "The client subcommand SYSCMD", (TCP, "QIBM_QTMF_FTP_CLIENT_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_0", "FTP server logon", "Log on to the FTP server; needed for every other server request", (TCP, "QIBM_QTMF_FTP_SERVER"), AN),
    f("QIBM_QTMF_SERVER_REQ_1", "FTP server MKD", "The server subcommand MKD", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_10", "FTP server CCC", "The server subcommand CCC", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), DN),
    f("QIBM_QTMF_SERVER_REQ_2", "FTP server RMD", "The server subcommand RMD", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_3", "FTP server CWD", "The server subcommand CWD", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_4", "FTP server LIST", "The server subcommands LIST and NLST", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_5", "FTP server DELE", "The server subcommand DELE", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_6", "FTP server RETR", "The server subcommand RETR", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_7", "FTP server STOR", "The server subcommands STOR, STOU and APPE", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_8", "FTP server rename", "The server subcommands RNFR and RNTO", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QTMF_SERVER_REQ_9", "FTP server RCMD", "The server subcommand RCMD", (TCP, "QIBM_QTMF_FTP_SERVER_REQ"), AN),
    f("QIBM_QYPS_MGTCTRL_SUPER_USER", "Management Central administration", "Every Management Central task, definition and monitor", ("QIBM_QYPS_MGTCTRL", "*NONE"), DN),
];

/// The registry: the functions with what was changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    /// Changed defaults: function, (allowed, `*ALLOBJ` used).
    pub settings: BTreeMap<String, (bool, bool)>,
    /// Usage: function, then profile, allowed.
    pub usage: BTreeMap<String, BTreeMap<String, bool>>,
}

/// A function as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Current {
    /// The function.
    pub function: &'static Function,
    /// Its default is allowed.
    pub default_allowed: bool,
    /// `*ALLOBJ` allows it.
    pub allobj_used: bool,
    /// The profiles with usage of their own, by name.
    pub users: Vec<(String, bool)>,
}

/// A change `CHGFCNUSG` makes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// A profile's usage: `Some(allowed)`, `None` to remove it (`*NONE`).
    pub user: Option<(String, Option<bool>)>,
    /// The default.
    pub default_allowed: Option<bool>,
    /// The `*ALLOBJ` use.
    pub allobj_used: Option<bool>,
}

/// The function `id`.
#[must_use]
pub fn function(id: &str) -> Option<&'static Function> {
    FUNCTIONS.iter().find(|f| f.id.eq_ignore_ascii_case(id))
}

fn not_valid(id: &str) -> SecError {
    SecError::msg("CPF228C", format!("Function ID {id} is not a function this system registers."))
}

/// Whether `id` matches `pattern`: `*ALL`, a generic `QIBM_DB*`, or the
/// ID itself.
#[must_use]
pub fn matches(pattern: &str, id: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    if p == "*ALL" || p.is_empty() {
        return true;
    }
    match p.strip_suffix('*') {
        Some(stem) => id.starts_with(stem),
        None => id == p,
    }
}

impl Registry {
    fn decode(text: &str) -> Self {
        let mut r = Self::default();
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["F", id, d, a] => {
                    r.settings
                        .insert((*id).to_string(), (*d == "ALLOWED", *a == "USED"));
                }
                ["U", id, user, u] => {
                    r.usage
                        .entry((*id).to_string())
                        .or_default()
                        .insert((*user).to_string(), *u == "ALLOWED");
                }
                _ => {}
            }
        }
        r
    }

    fn encode(&self) -> String {
        let mut s = String::new();
        for (id, (d, a)) in &self.settings {
            let _ = writeln!(
                s,
                "F {id} {} {}",
                if *d { "ALLOWED" } else { "DENIED" },
                if *a { "USED" } else { "NOTUSED" }
            );
        }
        for (id, users) in &self.usage {
            for (u, ok) in users {
                let _ = writeln!(s, "U {id} {u} {}", if *ok { "ALLOWED" } else { "DENIED" });
            }
        }
        s
    }

    /// The function `id` as it stands, `CPF228C` when it is not one.
    pub fn current(&self, id: &str) -> Result<Current> {
        let function = function(id).ok_or_else(|| not_valid(id))?;
        let (default_allowed, allobj_used) = self
            .settings
            .get(function.id)
            .copied()
            .unwrap_or((function.default_allowed, function.allobj_used));
        let users = self
            .usage
            .get(function.id)
            .map(|m| m.iter().map(|(u, a)| (u.clone(), *a)).collect())
            .unwrap_or_default();
        Ok(Current {
            function,
            default_allowed,
            allobj_used,
            users,
        })
    }

    /// The functions matching `pattern`, in the listing's order.
    #[must_use]
    pub fn list(&self, pattern: &str) -> Vec<Current> {
        FUNCTIONS
            .iter()
            .filter(|f| matches(pattern, f.id))
            .filter_map(|f| self.current(f.id).ok())
            .collect()
    }

    /// Apply `CHGFCNUSG`'s change to `id`.
    pub fn change(&mut self, id: &str, c: &Change) -> Result<()> {
        let cur = self.current(id)?;
        let fid = cur.function.id.to_string();
        if c.default_allowed.is_some() || c.allobj_used.is_some() {
            let d = c.default_allowed.unwrap_or(cur.default_allowed);
            let a = c.allobj_used.unwrap_or(cur.allobj_used);
            if (d, a) == (cur.function.default_allowed, cur.function.allobj_used) {
                self.settings.remove(&fid);
            } else {
                self.settings.insert(fid.clone(), (d, a));
            }
        }
        if let Some((user, usage)) = &c.user {
            let m = self.usage.entry(fid.clone()).or_default();
            match usage {
                Some(a) => {
                    m.insert(user.to_ascii_uppercase(), *a);
                }
                None => {
                    m.remove(&user.to_ascii_uppercase());
                }
            }
            if m.is_empty() {
                self.usage.remove(&fid);
            }
        }
        Ok(())
    }

    /// Forget a deleted profile's usage.
    pub fn forget(&mut self, user: &str) {
        for m in self.usage.values_mut() {
            m.remove(user);
        }
        self.usage.retain(|_, m| !m.is_empty());
    }

    /// Whether `who` may use `id`; `CPF228C` when it is not a function.
    pub fn allowed(&self, who: &Principal, id: &str) -> Result<bool> {
        let cur = self.current(id)?;
        let own = |name: &str| cur.users.iter().find(|(u, _)| u == name).map(|(_, a)| *a);
        if let Some(a) = own(&who.user) {
            return Ok(a);
        }
        let groups: Vec<bool> = who.groups.iter().filter_map(|g| own(g)).collect();
        if groups.contains(&true) {
            return Ok(true);
        }
        if !groups.is_empty() {
            return Ok(false);
        }
        if cur.allobj_used && who.has(SpecialAuth::ALLOBJ) {
            return Ok(true);
        }
        Ok(cur.default_allowed)
    }
}

/// The registry in the store.
pub fn load(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Registry> {
    Ok(crate::textspace::load(txn, qsys, SPACE.0, HEADER)?
        .map(|t| Registry::decode(&t))
        .unwrap_or_default())
}

/// Write the registry back.
pub fn save(txn: &mut Txn<'_>, qsys: ObjectId, r: &Registry) -> Result<()> {
    crate::textspace::save(txn, qsys, SPACE, HEADER, &r.encode(), crate::textspace::now())
}

/// Whether `who` may use function `id`, from the store.
pub fn check(txn: &mut Txn<'_>, qsys: ObjectId, who: &Principal, id: &str) -> Result<bool> {
    load(txn, qsys)?.allowed(who, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who(user: &str, groups: &[&str], allobj: bool) -> Principal {
        Principal {
            user: user.into(),
            groups: groups.iter().map(|g| (*g).to_string()).collect(),
            spcaut: if allobj { SpecialAuth::ALLOBJ.0 } else { 0 },
            ..Principal::default()
        }
    }

    #[test]
    fn the_primary_s_37_functions() {
        assert_eq!(FUNCTIONS.len(), 37);
        assert_eq!(FUNCTIONS[0].id, "QIBM_DIRSRV_ADMIN");
        assert!(function("qibm_db_sqladm").is_some());
    }

    #[test]
    fn user_then_group_then_allobj_then_default() {
        let mut r = Registry::default();
        let sqladm = "QIBM_DB_SQLADM";
        assert!(!r.allowed(&who("ANNE", &[], false), sqladm).unwrap());
        r.change(sqladm, &Change { user: Some(("DBA".into(), Some(true))), ..Change::default() }).unwrap();
        assert!(r.allowed(&who("ANNE", &["DBA"], false), sqladm).unwrap());
        r.change(sqladm, &Change { user: Some(("ANNE".into(), Some(false))), ..Change::default() }).unwrap();
        assert!(!r.allowed(&who("ANNE", &["DBA"], false), sqladm).unwrap());
        let joblog = "QIBM_ACCESS_ALLOBJ_JOBLOG";
        assert!(r.allowed(&who("SEC", &[], true), joblog).unwrap());
        r.change(joblog, &Change { allobj_used: Some(false), ..Change::default() }).unwrap();
        assert!(!r.allowed(&who("SEC", &[], true), joblog).unwrap());
        assert_eq!(r.allowed(&who("X", &[], false), "QIBM_NOPE").unwrap_err().msgid(), Some("CPF228C"));
        let back = Registry::decode(&r.encode());
        assert_eq!(back, r);
    }

    #[test]
    fn generic_names() {
        assert!(matches("QIBM_DB*", "QIBM_DB_SYSMON"));
        assert!(!matches("QIBM_DB*", "QIBM_DIRSRV_ADMIN"));
        assert!(matches("*ALL", "QIBM_DIRSRV_ADMIN"));
    }
}
