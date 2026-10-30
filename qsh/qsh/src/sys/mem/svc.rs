// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The in-memory system's stand-ins for the services: DNS records, CL
//! and SQL handlers the tests supply, a connector, and tables of
//! library list entries, system values, data areas and queues, spooled
//! files, IPC objects and attributes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::{Fd, Kind, MEM_PID, MEM_UID, MemSys, lock};
use crate::sys::services::{rcode, rtype};
use crate::sys::{
    ClOutcome, ClRequest, ClRunner, Credentials, DataArea, DnsAnswer, DnsQuery, DnsRecord,
    DtaaraType, DtaqOp, DtaqReply, Errno, IpcInfo, IpcKind, IpcTarget, LibEntry, LibKind,
    LiblChange, Resolver, SplfInfo, SplfNumber, Spooled, SqlOutcome, SqlRequest, SqlRunner,
    SysError,
};

/// A test's CL command handler.
pub type ClHandler = Arc<dyn Fn(&ClRequest) -> ClOutcome + Send + Sync>;
/// A test's SQL statement handler.
pub type SqlHandler = Arc<dyn Fn(&SqlRequest) -> SqlOutcome + Send + Sync>;
/// A test's TCP connector.
pub type Connector = Arc<dyn Fn(&str, u16) -> Result<Fd, SysError> + Send + Sync>;

/// The job the shell runs in.
pub const MEM_JOB: &str = "000123/QSHUSER/QZSHSH";

type Queue = Vec<(Option<Vec<u8>>, Vec<u8>)>;
type JobSplfs = Vec<(SplfInfo, Vec<Vec<String>>)>;

pub(super) struct Services {
    dns: Mutex<Vec<DnsRecord>>,
    dns_server: Mutex<String>,
    dns_domain: Mutex<String>,
    dns_auth: Mutex<bool>,
    host_name: Mutex<String>,
    cl: Mutex<Option<ClHandler>>,
    sql: Mutex<Option<SqlHandler>>,
    connect: Mutex<Option<Connector>>,
    creds: Mutex<Credentials>,
    cwds: Mutex<BTreeMap<i32, String>>,
    attrs: Mutex<BTreeMap<usize, BTreeMap<String, String>>>,
    libl: Mutex<Vec<LibEntry>>,
    sysvals: Mutex<BTreeMap<String, String>>,
    netattrs: Mutex<BTreeMap<String, String>>,
    dtaaras: Mutex<BTreeMap<String, DataArea>>,
    dtaqs: Mutex<BTreeMap<String, Queue>>,
    splfs: Mutex<BTreeMap<String, JobSplfs>>,
    ipcs: Mutex<Vec<IpcInfo>>,
}

fn lib(name: &str, kind: LibKind) -> LibEntry {
    LibEntry {
        name: name.to_string(),
        kind,
    }
}

impl Services {
    pub(super) fn new() -> Self {
        let blank = |n: usize| DataArea {
            kind: DtaaraType::Char,
            length: n,
            value: " ".repeat(n),
        };
        Self {
            dns: Mutex::new(Vec::new()),
            dns_server: Mutex::new("10.0.0.53".to_string()),
            dns_domain: Mutex::new("example.com".to_string()),
            dns_auth: Mutex::new(false),
            host_name: Mutex::new("QS400.EXAMPLE.COM".to_string()),
            cl: Mutex::new(None),
            sql: Mutex::new(None),
            connect: Mutex::new(None),
            creds: Mutex::new(Credentials {
                user: "QSHUSER".to_string(),
                uid: MEM_UID,
                gid: 0,
                groups: Vec::new(),
            }),
            cwds: Mutex::new(BTreeMap::new()),
            attrs: Mutex::new(BTreeMap::new()),
            libl: Mutex::new(vec![
                lib("QSYS", LibKind::System),
                lib("QSYS2", LibKind::System),
                lib("QHLPSYS", LibKind::System),
                lib("QUSRSYS", LibKind::System),
                lib("QGPL", LibKind::User),
                lib("QTEMP", LibKind::User),
            ]),
            sysvals: Mutex::new(BTreeMap::new()),
            netattrs: Mutex::new(BTreeMap::new()),
            dtaaras: Mutex::new(BTreeMap::from([("*LDA".to_string(), blank(1024))])),
            dtaqs: Mutex::new(BTreeMap::new()),
            splfs: Mutex::new(BTreeMap::new()),
            ipcs: Mutex::new(Vec::new()),
        }
    }
}

fn job_key(job: &str) -> String {
    if job == "*" {
        MEM_JOB.to_string()
    } else {
        job.to_ascii_uppercase()
    }
}

fn dec_text(v: &str, digits: u32, decimals: u32) -> Option<String> {
    let t = v.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (int, frac) = t.split_once('.').unwrap_or((t, ""));
    if (int.is_empty() && frac.is_empty())
        || !int.bytes().all(|b| b.is_ascii_digit())
        || !frac.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let int = int.trim_start_matches('0');
    if int.len() > (digits - decimals) as usize || frac.len() > decimals as usize {
        return None;
    }
    let mut frac = frac.to_string();
    while frac.len() < decimals as usize {
        frac.push('0');
    }
    let int = if int.is_empty() { "0" } else { int };
    let zero = int == "0" && frac.bytes().all(|b| b == b'0');
    let sign = if neg && !zero { "-" } else { "" };
    Some(if decimals == 0 {
        format!("{sign}{int}")
    } else {
        format!("{sign}{int}.{frac}")
    })
}

/// Test setters for the services.
impl MemSys {
    /// Add a DNS record (owner name without a final dot).
    pub fn add_dns(&self, name: &str, rtype: u16, ttl: u32, data: &str) {
        lock(&self.0.svc.dns).push(DnsRecord {
            name: name.trim_end_matches('.').to_string(),
            rtype,
            ttl,
            data: data.to_string(),
        });
    }

    /// Set the DNS server address and the search domain.
    pub fn set_dns_server(&self, server: &str, domain: &str) {
        *lock(&self.0.svc.dns_server) = server.to_string();
        *lock(&self.0.svc.dns_domain) = domain.to_string();
    }

    /// Answer as an authoritative server.
    pub fn set_dns_authoritative(&self, auth: bool) {
        *lock(&self.0.svc.dns_auth) = auth;
    }

    /// Set the TCP/IP host name `hostname` prints.
    pub fn set_host_name(&self, name: &str) {
        *lock(&self.0.svc.host_name) = name.to_string();
    }

    /// Answer CL commands with a handler.
    pub fn set_cl(&self, h: ClHandler) {
        *lock(&self.0.svc.cl) = Some(h);
    }

    /// Answer SQL statements with a handler.
    pub fn set_sql(&self, h: SqlHandler) {
        *lock(&self.0.svc.sql) = Some(h);
    }

    /// Answer TCP connections with a connector.
    pub fn set_connector(&self, c: Connector) {
        *lock(&self.0.svc.connect) = Some(c);
    }

    /// Set who the job runs as.
    pub fn set_credentials(&self, c: Credentials) {
        *lock(&self.0.svc.creds) = c;
    }

    /// Set a process's working directory for `pwdx`.
    pub fn set_process_cwd(&self, pid: i32, cwd: &str) {
        lock(&self.0.svc.cwds).insert(pid, cwd.to_string());
    }

    /// Set a system value.
    pub fn set_sysval(&self, name: &str, value: &str) {
        lock(&self.0.svc.sysvals).insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// Set a network attribute.
    pub fn set_netattr(&self, name: &str, value: &str) {
        lock(&self.0.svc.netattrs).insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// Make a data area (`*LDA` or a `/QSYS.LIB` path), replacing one.
    pub fn set_data_area(&self, name: &str, area: DataArea) {
        lock(&self.0.svc.dtaaras).insert(name.to_ascii_uppercase(), area);
    }

    /// Make an empty data queue at a `/QSYS.LIB` path.
    pub fn make_data_queue(&self, path: &str) {
        lock(&self.0.svc.dtaqs).insert(path.to_ascii_uppercase(), Vec::new());
    }

    /// Add a spooled file to a job (`*` the shell's job).
    pub fn add_spooled_file(&self, job: &str, info: SplfInfo, pages: Vec<Vec<String>>) {
        lock(&self.0.svc.splfs)
            .entry(job_key(job))
            .or_default()
            .push((info, pages));
    }

    /// Add an IPC object.
    pub fn add_ipc(&self, info: IpcInfo) {
        lock(&self.0.svc.ipcs).push(info);
    }

    /// Replace the library list.
    pub fn set_library_list(&self, list: Vec<LibEntry>) {
        *lock(&self.0.svc.libl) = list;
    }

    // The `Sys` methods' bodies.

    pub(super) fn svc_connect(&self, host: &str, port: u16) -> Result<Fd, SysError> {
        let c = lock(&self.0.svc.connect).clone();
        match c {
            Some(c) => c(host, port),
            None => Err(Errno::ECONNREFUSED.into()),
        }
    }

    pub(super) fn svc_credentials(&self) -> Credentials {
        lock(&self.0.svc.creds).clone()
    }

    pub(super) fn svc_process_cwd(&self, pid: i32) -> Result<String, SysError> {
        if pid == MEM_PID {
            return Ok(lock(&self.0.cwd).clone());
        }
        lock(&self.0.svc.cwds)
            .get(&pid)
            .cloned()
            .ok_or(Errno::ESRCH.into())
    }

    fn default_attrs(&self, i: usize, canon: &str) -> BTreeMap<String, String> {
        let fs = lock(&self.0.fs);
        let st = fs.stat_of(i);
        let up = canon.to_ascii_uppercase();
        let qsys = up.starts_with("/QSYS.LIB");
        let objtype = match &fs.nodes[i].kind {
            Kind::Dir(_) if qsys => up.rsplit('.').next().map_or("*DIR".to_string(), |t| {
                if up == "/QSYS.LIB" {
                    "*LIB".to_string()
                } else {
                    format!("*{t}")
                }
            }),
            Kind::Dir(_) => "*DIR".to_string(),
            Kind::File(_) if qsys => "*MBR".to_string(),
            Kind::File(_) => "*STMF".to_string(),
            Kind::Symlink(_) => "*SYMLNK".to_string(),
            Kind::Null => "*CHRSF".to_string(),
            Kind::Fifo => "*FIFO".to_string(),
            Kind::Object(_) => up.rsplit('.').next().map_or("*OBJ".to_string(), |t| format!("*{t}")),
        };
        let ccsid = if qsys { "37" } else { "819" };
        let yes_no = |b: bool| if b { "YES" } else { "NO" }.to_string();
        let mut m = BTreeMap::new();
        m.insert("OBJTYPE".to_string(), objtype);
        m.insert("CCSID".to_string(), ccsid.to_string());
        m.insert("CODEPAGE".to_string(), ccsid.to_string());
        m.insert("DATA_SIZE".to_string(), st.size.to_string());
        m.insert("ALLOC_SIZE".to_string(), (st.size.div_ceil(4096) * 4096).to_string());
        m.insert("ASP".to_string(), "1".to_string());
        m.insert("ALWSAV".to_string(), yes_no(true));
        m.insert("PC_READ_ONLY".to_string(), yes_no(st.mode & 0o222 == 0));
        m.insert("PC_HIDDEN".to_string(), yes_no(false));
        m.insert("PC_SYSTEM".to_string(), yes_no(false));
        m.insert("PC_ARCHIVE".to_string(), yes_no(true));
        m.insert("SYS_ARCHIVE".to_string(), yes_no(false));
        m
    }

    fn attr_node(&self, path: &str, follow: bool) -> Result<(usize, String), SysError> {
        lock(&self.0.fs).resolve(path, follow, 0)
    }

    pub(super) fn svc_attributes(
        &self,
        path: &str,
        follow: bool,
    ) -> Result<Vec<(String, String)>, SysError> {
        let (i, canon) = self.attr_node(path, follow)?;
        let mut m = self.default_attrs(i, &canon);
        if let Some(over) = lock(&self.0.svc.attrs).get(&i) {
            for (k, v) in over {
                m.insert(k.clone(), v.clone());
            }
        }
        Ok(m.into_iter().collect())
    }

    pub(super) fn svc_set_attribute(
        &self,
        path: &str,
        follow: bool,
        name: &str,
        value: &str,
    ) -> Result<(), SysError> {
        let (i, canon) = self.attr_node(path, follow)?;
        let name = name.to_ascii_uppercase();
        let defaults = self.default_attrs(i, &canon);
        if !defaults.contains_key(&name) {
            return Err(Errno::EINVAL.into());
        }
        let value = value.to_ascii_uppercase();
        let mut set = vec![(name.clone(), value.clone())];
        match name.as_str() {
            "OBJTYPE" | "DATA_SIZE" | "ALLOC_SIZE" | "ASP" | "CODEPAGE" => {
                return Err(Errno::EPERM.into());
            }
            "CCSID" => {
                if value.parse::<u16>().is_err() {
                    return Err(Errno::EINVAL.into());
                }
                set.push(("CODEPAGE".to_string(), value));
            }
            _ => {
                if value != "YES" && value != "NO" {
                    return Err(Errno::EINVAL.into());
                }
            }
        }
        let mut attrs = lock(&self.0.svc.attrs);
        let e = attrs.entry(i).or_default();
        for (k, v) in set {
            e.insert(k, v);
        }
        Ok(())
    }

    pub(super) fn svc_library_list(&self) -> Vec<LibEntry> {
        lock(&self.0.svc.libl).clone()
    }

    pub(super) fn svc_change_libl(&self, ch: &LiblChange) -> Result<(), SysError> {
        let mut l = lock(&self.0.svc.libl);
        let user_at = l
            .iter()
            .position(|e| e.kind == LibKind::User)
            .unwrap_or(l.len());
        let has = |l: &[LibEntry], n: &str| l.iter().any(|e| e.name.eq_ignore_ascii_case(n));
        match ch {
            LiblChange::AddFirst(n) | LiblChange::AddLast(n) => {
                if has(&l, n) {
                    return Err(Errno::EEXIST.into());
                }
                let e = lib(&n.to_ascii_uppercase(), LibKind::User);
                if matches!(ch, LiblChange::AddFirst(_)) {
                    l.insert(user_at, e);
                } else {
                    l.push(e);
                }
            }
            LiblChange::Remove(n) => {
                let Some(i) = l
                    .iter()
                    .position(|e| e.kind == LibKind::User && e.name.eq_ignore_ascii_case(n))
                else {
                    return Err(Errno::ENOENT.into());
                };
                l.remove(i);
            }
            LiblChange::SetCurrent(n) => {
                l.retain(|e| e.kind != LibKind::Current);
                if !n.eq_ignore_ascii_case("*CRTDFT") {
                    let at = l
                        .iter()
                        .position(|e| e.kind == LibKind::User)
                        .unwrap_or(l.len());
                    l.insert(at, lib(&n.to_ascii_uppercase(), LibKind::Current));
                }
            }
        }
        Ok(())
    }

    pub(super) fn svc_sysval(&self, name: &str, net: bool) -> Result<String, SysError> {
        let m = if net {
            lock(&self.0.svc.netattrs)
        } else {
            lock(&self.0.svc.sysvals)
        };
        m.get(&name.to_ascii_uppercase())
            .cloned()
            .ok_or(Errno::ENOENT.into())
    }

    pub(super) fn svc_read_dtaara(&self, name: &str) -> Result<DataArea, SysError> {
        lock(&self.0.svc.dtaaras)
            .get(&name.to_ascii_uppercase())
            .cloned()
            .ok_or(Errno::ENOENT.into())
    }

    pub(super) fn svc_write_dtaara(
        &self,
        name: &str,
        part: Option<(usize, usize)>,
        value: &str,
    ) -> Result<(), SysError> {
        let mut m = lock(&self.0.svc.dtaaras);
        let a = m
            .get_mut(&name.to_ascii_uppercase())
            .ok_or(SysError::new(Errno::ENOENT))?;
        match a.kind {
            DtaaraType::Char => {
                let mut cur: Vec<char> = a.value.chars().collect();
                cur.resize(a.length, ' ');
                let (off, len) = part.unwrap_or((0, a.length));
                if off + len > a.length {
                    return Err(Errno::EINVAL.into());
                }
                let mut v: Vec<char> = value.chars().take(len).collect();
                v.resize(len, ' ');
                cur.splice(off..off + len, v);
                a.value = cur.into_iter().collect();
            }
            DtaaraType::Dec(d, p) => {
                if part.is_some() {
                    return Err(Errno::EINVAL.into());
                }
                a.value = dec_text(value, d, p).ok_or(SysError::new(Errno::EINVAL))?;
            }
            DtaaraType::Logical => {
                if part.is_some() || (value != "0" && value != "1") {
                    return Err(Errno::EINVAL.into());
                }
                a.value = value.to_string();
            }
        }
        Ok(())
    }

    pub(super) fn svc_dtaq(&self, path: &str, op: &DtaqOp) -> Result<DtaqReply, SysError> {
        let mut m = lock(&self.0.svc.dtaqs);
        let q = m
            .get_mut(&path.to_ascii_uppercase())
            .ok_or(SysError::new(Errno::ENOENT))?;
        Ok(match op {
            DtaqOp::Send { data, key } => {
                q.push((key.clone(), data.clone()));
                DtaqReply::Done
            }
            DtaqOp::Receive { key, wait, remove } => {
                let at = q.iter().position(|(k, _)| key.is_none() || k == key);
                match at {
                    Some(i) => {
                        let data = q[i].1.clone();
                        if *remove {
                            q.remove(i);
                        }
                        DtaqReply::Entry(Some(data))
                    }
                    None => {
                        drop(m);
                        if let Some(w) = wait {
                            self.advance(*w);
                        }
                        DtaqReply::Entry(None)
                    }
                }
            }
            DtaqOp::Clear => {
                q.clear();
                DtaqReply::Done
            }
            DtaqOp::Count => DtaqReply::Count(q.len() as u64),
        })
    }

    pub(super) fn svc_splfs(&self, job: &str) -> Result<Vec<SplfInfo>, SysError> {
        let key = job_key(job);
        let m = lock(&self.0.svc.splfs);
        match m.get(&key) {
            Some(v) => Ok(v.iter().map(|(i, _)| i.clone()).collect()),
            None if key == MEM_JOB => Ok(Vec::new()),
            None => Err(Errno::ESRCH.into()),
        }
    }

    pub(super) fn svc_splf(
        &self,
        job: &str,
        name: &str,
        number: SplfNumber,
    ) -> Result<Spooled, SysError> {
        let key = job_key(job);
        let m = lock(&self.0.svc.splfs);
        let v = match m.get(&key) {
            Some(v) => v,
            None if key == MEM_JOB => return Err(Errno::ENOENT.into()),
            None => return Err(Errno::ESRCH.into()),
        };
        let mut named: Vec<&(SplfInfo, Vec<Vec<String>>)> = v
            .iter()
            .filter(|(i, _)| i.name.eq_ignore_ascii_case(name))
            .collect();
        named.sort_by_key(|(i, _)| i.number);
        let pick = match number {
            SplfNumber::Number(n) => named.into_iter().find(|(i, _)| i.number == n),
            SplfNumber::Last => named.last().copied(),
            SplfNumber::Only => {
                if named.len() > 1 {
                    return Err(Errno::EINVAL.into());
                }
                named.first().copied()
            }
        };
        let (info, pages) = pick.ok_or(SysError::new(Errno::ENOENT))?;
        Ok(Spooled {
            name: info.name.clone(),
            pages: pages.clone(),
        })
    }

    pub(super) fn svc_ipc_list(&self) -> Vec<IpcInfo> {
        lock(&self.0.svc.ipcs).clone()
    }

    pub(super) fn svc_ipc_remove(&self, kind: IpcKind, t: IpcTarget) -> Result<(), SysError> {
        let mut v = lock(&self.0.svc.ipcs);
        let at = v.iter().position(|i| {
            i.kind == kind
                && match t {
                    IpcTarget::Id(n) => i.id == n,
                    IpcTarget::Key(k) => i.key == k && k != 0,
                }
        });
        match at {
            Some(i) => {
                v.remove(i);
                Ok(())
            }
            None => Err(Errno::EINVAL.into()),
        }
    }
}

fn find_dns(all: &[DnsRecord], name: &str) -> Vec<DnsRecord> {
    all.iter()
        .filter(|r| r.name.eq_ignore_ascii_case(name))
        .cloned()
        .collect()
}

impl Resolver for MemSys {
    fn query(&self, q: &DnsQuery) -> Result<DnsAnswer, SysError> {
        let all = lock(&self.0.svc.dns).clone();
        let domain = lock(&self.0.svc.dns_domain).clone();
        let mut name = q.name.trim_end_matches('.').to_string();
        // A relative name with no records of its own takes the domain.
        if !q.name.ends_with('.') && find_dns(&all, &name).is_empty() && !domain.is_empty() {
            let full = format!("{name}.{domain}");
            if !find_dns(&all, &full).is_empty() {
                name = full;
            }
        }
        let mut ans = DnsAnswer {
            server: q
                .server
                .clone()
                .unwrap_or_else(|| lock(&self.0.svc.dns_server).clone()),
            port: 53,
            rcode: rcode::NOERROR,
            authoritative: *lock(&self.0.svc.dns_auth),
            qname: name.clone(),
            ..DnsAnswer::default()
        };
        let mut cur = name;
        for _ in 0..8 {
            let own = find_dns(&all, &cur);
            if own.is_empty() {
                if ans.answers.is_empty() {
                    ans.rcode = rcode::NXDOMAIN;
                }
                break;
            }
            let hits: Vec<DnsRecord> = own
                .iter()
                .filter(|r| q.rtype == rtype::ANY || r.rtype == q.rtype)
                .cloned()
                .collect();
            if !hits.is_empty() {
                ans.answers.extend(hits);
                break;
            }
            match own.iter().find(|r| r.rtype == rtype::CNAME) {
                Some(c) => {
                    ans.answers.push(c.clone());
                    cur = c.data.trim_end_matches('.').to_string();
                }
                None => break,
            }
        }
        Ok(ans)
    }

    fn host_name(&self) -> Result<String, SysError> {
        Ok(lock(&self.0.svc.host_name).clone())
    }
}

impl ClRunner for MemSys {
    fn run_cl(&self, req: &ClRequest) -> Result<ClOutcome, SysError> {
        let h = lock(&self.0.svc.cl).clone();
        match h {
            Some(h) => Ok(h(req)),
            None => Err(Errno::ENOSYS.into()),
        }
    }
}

impl SqlRunner for MemSys {
    fn run_sql(&self, req: &SqlRequest) -> Result<SqlOutcome, SysError> {
        let h = lock(&self.0.svc.sql).clone();
        match h {
            Some(h) => Ok(h(req)),
            None => Err(Errno::ENOSYS.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::dec_text;

    #[test]
    fn decimals() {
        assert_eq!(dec_text("12.5", 5, 2).as_deref(), Some("12.50"));
        assert_eq!(dec_text("-0", 5, 0).as_deref(), Some("0"));
        assert_eq!(dec_text("1234", 5, 2), None);
        assert_eq!(dec_text("1.234", 5, 2), None);
        assert_eq!(dec_text("x", 5, 2), None);
    }
}
