// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job as the shell sees it ([`qsh_job::Side`]): the IFS over the
//! job's link, CL through the command analyzer, SQL in the job's SQL
//! session, programs called in the job, child jobs, the resolver, the
//! library list, system values, data areas, spooled files and the
//! active jobs.

use std::rc::Rc;

use ifs::client::IfsClient;
use mi::Job;
use qsh::sys::{
    Child, ClMessage, ClOutcome, ClRequest, Credentials, DataArea, DnsAnswer, DnsQuery, DnsRecord,
    DtaaraType, Errno, LibEntry, LibKind, LiblChange, MsgKind, ProcInfo, SqlColumn, SqlOutcome,
    SqlRequest, SysError,
};
use qsh_job::{Ids, JobSpawn, ProgramRun, Side};
use stg::Client;
use stg::qsh::{QshOp, QshReply};

use crate::console::host::Host;

/// The job's side of its shells.
pub struct JobSide<'a> {
    /// The ILE job.
    pub job: &'a mut Job,
    /// Its surroundings.
    pub host: &'a Host,
    ifs: Rc<dyn IfsClient>,
    sql_started: bool,
}

impl<'a> JobSide<'a> {
    /// Over a job and its host.
    pub fn new(job: &'a mut Job, host: &'a Host) -> Self {
        Self {
            ifs: Rc::new(stg::remote::RemoteIfs::new(host.link.clone())),
            job,
            host,
            sql_started: false,
        }
    }
}

/// The profiles' IDs from the service.
pub fn profiles(host: &Host) -> Vec<Ids> {
    match host.link.borrow_mut().with(|l| l.qsh(QshOp::Profiles)) {
        QshReply::Profiles { rows } => rows
            .into_iter()
            .map(|r| Ids {
                name: r.name,
                uid: r.uid,
                gid: r.gid,
                home: r.home,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A command's output lines as messages: `CPF1234 text` lines with
/// their identifiers, the escape last.
fn messages(text: &str, escape: Option<&crate::command::Fail>) -> Vec<ClMessage> {
    let mut v = Vec::new();
    for l in text.lines().filter(|l| !l.trim().is_empty()) {
        let (id, rest) = match l.split_once(' ') {
            Some((w, r)) if crate::command::jobsite::leading_msgid(w).is_some() => (w, r),
            _ => ("", l),
        };
        if escape.is_some_and(|f| f.msgid == id) {
            continue;
        }
        v.push(ClMessage {
            id: id.to_string(),
            kind: MsgKind::Completion,
            text: rest.trim().to_string(),
        });
    }
    if let Some(f) = escape {
        let text = f.text.lines().last().unwrap_or("").trim();
        let text = text
            .strip_prefix(f.msgid.as_str())
            .map_or(text, str::trim)
            .to_string();
        v.push(ClMessage {
            id: f.msgid.clone(),
            kind: MsgKind::Escape,
            text,
        });
    }
    v
}

/// A packed decimal as text: sign, digits, the point.
fn unpack(b: &[u8], scale: u32) -> String {
    let mut digits = String::new();
    for (i, x) in b.iter().enumerate() {
        digits.push(char::from(b'0' + (x >> 4)));
        if i + 1 < b.len() {
            digits.push(char::from(b'0' + (x & 0x0f)));
        }
    }
    let neg = b.last().is_some_and(|x| matches!(x & 0x0f, 0x0b | 0x0d));
    let scale = usize::try_from(scale).unwrap_or(0).min(digits.len());
    let (int, frac) = digits.split_at(digits.len() - scale);
    let int = int.trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(int);
    if scale > 0 {
        out.push('.');
        out.push_str(frac);
    }
    out
}

fn numeric(sql_type: &str) -> bool {
    let t = sql_type.to_ascii_uppercase();
    ["INT", "DEC", "NUMERIC", "FLOAT", "DOUBLE", "REAL", "BIGINT", "SMALLINT"]
        .iter()
        .any(|k| t.contains(k))
}

fn rdata(d: &sockets::resolver::codec::Rdata) -> String {
    use sockets::resolver::codec::Rdata;
    match d {
        Rdata::A(a) => a.to_string(),
        Rdata::Aaaa(a) => a.to_string(),
        Rdata::Cname(n) | Rdata::Ptr(n) => format!("{}.", n.trim_end_matches('.')),
        Rdata::Other(b) => {
            use std::fmt::Write as _;
            b.iter().fold(String::new(), |mut s, x| {
                let _ = write!(s, "{x:02x}");
                s
            })
        }
    }
}

fn records(v: &[sockets::resolver::codec::Record]) -> Vec<DnsRecord> {
    v.iter()
        .map(|r| DnsRecord {
            name: r.name.trim_end_matches('.').to_string(),
            rtype: r.rtype,
            ttl: r.ttl,
            data: rdata(&r.data),
        })
        .collect()
}

impl Side for JobSide<'_> {
    fn ifs(&self) -> &dyn IfsClient {
        &*self.ifs
    }

    fn run_cl(&mut self, req: &ClRequest) -> Result<ClOutcome, SysError> {
        for (k, v) in &req.env {
            let _ = sysapi::envproc::set_job(
                sysapi::envproc::Var {
                    name: k.clone(),
                    value: Some(v.clone()),
                    ccsid: 0,
                },
                sysapi::envproc::SetMode::Put,
            );
        }
        let o = crate::command::run(self.job, self.host, &req.command);
        Ok(ClOutcome {
            messages: messages(&o.text, o.escape.as_ref()),
            spooled: Vec::new(),
            failed: o.escape.is_some(),
        })
    }

    fn run_sql(&mut self, req: &SqlRequest) -> Result<SqlOutcome, SysError> {
        let naming = if req.system_naming { "*SYS" } else { "*SQL" };
        if !self.sql_started {
            let _ = self
                .host
                .link
                .borrow_mut()
                .with(|l| l.isql_start(naming.into(), "*NONE".into()));
            self.sql_started = true;
        }
        let text = req.statement.clone();
        let r = self
            .host
            .link
            .borrow_mut()
            .with(|l| l.isql_run(text, naming.into(), false, 100_000));
        if r.sqlcode < 0 {
            return Ok(SqlOutcome::Error {
                sqlstate: r.sqlstate,
                code: r.sqlcode,
                text: r.text,
            });
        }
        if r.columns.is_empty() {
            return Ok(SqlOutcome::Done {
                count: (r.count > 0 || r.statement != "SELECT").then_some(r.count),
            });
        }
        let columns = r
            .columns
            .iter()
            .enumerate()
            .map(|(i, (name, ty))| {
                let w = r
                    .rows
                    .iter()
                    .filter_map(|row| row.get(i).cloned().flatten())
                    .map(|v| v.chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(name.chars().count());
                SqlColumn {
                    name: name.clone(),
                    width: w,
                    numeric: numeric(ty),
                }
            })
            .collect();
        Ok(SqlOutcome::Rows {
            columns,
            rows: r.rows,
        })
    }

    fn call_program(&mut self, lib: &str, pgm: &str, args: &[String]) -> Result<ProgramRun, SysError> {
        let parms: Vec<String> = args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect();
        let line = if parms.is_empty() {
            format!("CALL PGM({lib}/{pgm})")
        } else {
            format!("CALL PGM({lib}/{pgm}) PARM({})", parms.join(" "))
        };
        let o = crate::command::run(self.job, self.host, &line);
        Ok(ProgramRun {
            status: i32::from(o.escape.is_some()),
            output: o.text,
        })
    }

    fn spawn_job(&mut self, req: JobSpawn) -> Result<Box<dyn Child>, SysError> {
        super::child::spawn_for_shell(self.host, &req)
    }

    fn signal_job(&mut self, pid: i32, sig: i32) -> Result<(), SysError> {
        super::child::signal(self.host, pid, sig).map_err(|e| SysError::new(Errno(e)))
    }

    fn processes(&mut self) -> Result<Vec<ProcInfo>, SysError> {
        let jobs = self.host.link.borrow_mut().with(Client::active_jobs);
        let ids = profiles(self.host);
        Ok(jobs
            .into_iter()
            .map(|j| ProcInfo {
                pid: i32::try_from(j.pid).unwrap_or(0),
                ppid: 1,
                pgid: i32::try_from(j.pid).unwrap_or(0),
                uid: ids
                    .iter()
                    .find(|i| i.name == j.user)
                    .map_or(0, |i| i.uid),
                job: j.qualified(),
                status: j.job_status().code().to_string(),
                function: j.function.clone(),
                device: "-".to_string(),
                cpu: std::time::Duration::ZERO,
                args: vec![j.name.clone()],
            })
            .collect())
    }

    fn credentials(&mut self) -> Result<Credentials, SysError> {
        let user = self.host.start.user.clone();
        let me = profiles(self.host).into_iter().find(|i| i.name == user);
        Ok(Credentials {
            user,
            uid: me.as_ref().map_or(0, |m| m.uid),
            gid: me.map_or(0, |m| m.gid),
            groups: Vec::new(),
        })
    }

    fn profiles(&mut self) -> Result<Vec<Ids>, SysError> {
        Ok(profiles(self.host))
    }

    fn resolve(&mut self, q: &DnsQuery) -> Result<DnsAnswer, SysError> {
        use sockets::resolver::HostError;
        use sockets::resolver::codec::Message;
        let net = sysapi::net::net(self.job);
        let mut n = net.borrow_mut();
        let server = n
            .resolver
            .domain()
            .servers
            .first()
            .map(ToString::to_string)
            .unwrap_or_default();
        let port = n.resolver.domain().port;
        match n.resolver.search(&q.name, 1, q.rtype) {
            Ok(bytes) => {
                let m = Message::decode(&bytes).map_err(|_| SysError::new(Errno::EIO))?;
                Ok(DnsAnswer {
                    server,
                    port,
                    rcode: m.header.rcode,
                    authoritative: m.header.aa,
                    qname: m
                        .questions
                        .first()
                        .map_or_else(|| q.name.clone(), |x| x.name.clone()),
                    answers: records(&m.answers),
                    authority: records(&m.authority),
                    additional: records(&m.additional),
                })
            }
            Err(HostError::NotFound | HostError::NoData) => Ok(DnsAnswer {
                server,
                port,
                rcode: 3,
                qname: q.name.clone(),
                ..DnsAnswer::default()
            }),
            Err(_) => Err(SysError::new(Errno::ETIMEDOUT)),
        }
    }

    fn host_name(&mut self) -> Result<String, SysError> {
        let net = sysapi::net::net(self.job);
        let n = net.borrow();
        let d = n.resolver.domain();
        if d.host_name.is_empty() {
            return Ok(crate::display::dds::system_name(self.host));
        }
        Ok(if d.domain_name.is_empty() {
            d.host_name.clone()
        } else {
            format!("{}.{}", d.host_name, d.domain_name)
        })
    }

    fn library_list(&mut self) -> Result<Vec<LibEntry>, SysError> {
        self.host.attrs_changed();
        let l = self.host.attrs().libl;
        let mut v = Vec::new();
        let mut add = |names: &[String], kind| {
            for n in names {
                v.push(LibEntry {
                    name: n.clone(),
                    kind,
                });
            }
        };
        add(&l.sys, LibKind::System);
        add(&l.prd, LibKind::Product);
        if let Some(c) = &l.cur {
            add(std::slice::from_ref(c), LibKind::Current);
        }
        add(&l.usr, LibKind::User);
        Ok(v)
    }

    fn change_library_list(&mut self, change: &LiblChange) -> Result<(), SysError> {
        let line = match change {
            LiblChange::AddFirst(l) => format!("ADDLIBLE LIB({l}) POSITION(*FIRST)"),
            LiblChange::AddLast(l) => format!("ADDLIBLE LIB({l}) POSITION(*LAST)"),
            LiblChange::Remove(l) => format!("RMVLIBLE LIB({l})"),
            LiblChange::SetCurrent(l) => format!("CHGCURLIB CURLIB({l})"),
        };
        let o = crate::command::run(self.job, self.host, &line);
        self.host.attrs_changed();
        if o.escape.is_some() {
            return Err(SysError::new(Errno::EINVAL));
        }
        Ok(())
    }

    fn system_value(&mut self, name: &str, net: bool) -> Result<String, SysError> {
        let n = name.to_ascii_uppercase();
        let v = if net {
            self.host.link.borrow_mut().with(|l| l.netattr(n))
        } else {
            self.host.link.borrow_mut().with(|l| l.sysval(n))
        };
        v.map(|v| ccsid::from_ebcdic(&v.data).trim_end().to_string())
            .ok_or_else(|| SysError::new(Errno::ENOENT))
    }

    fn read_data_area(&mut self, name: &str) -> Result<DataArea, SysError> {
        let up = name.to_ascii_uppercase();
        if up == "*LDA" {
            let b = self.host.lda.borrow().clone();
            return Ok(DataArea {
                kind: DtaaraType::Char,
                length: b.len(),
                value: ccsid::from_ebcdic(&b),
            });
        }
        let (lib, obj) = super::object_of(&up, ".DTAARA").ok_or_else(|| SysError::new(Errno::EINVAL))?;
        let v = self
            .host
            .link
            .borrow_mut()
            .with(|l| l.da_read(obj, lib))
            .map_err(|_| SysError::new(Errno::ENOENT))?;
        let kind = match v.kind {
            mi::io::DataAreaKind::Dec => DtaaraType::Dec(v.len, v.decimals),
            mi::io::DataAreaKind::Lgl => DtaaraType::Logical,
            mi::io::DataAreaKind::Char => DtaaraType::Char,
        };
        let value = match kind {
            DtaaraType::Dec(_, scale) => unpack(&v.value, scale),
            DtaaraType::Logical | DtaaraType::Char => ccsid::from_ebcdic(&v.value),
        };
        Ok(DataArea {
            kind,
            length: usize::try_from(v.len).unwrap_or(v.value.len()),
            value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_values() {
        assert_eq!(unpack(&[0x01, 0x23, 0x4f], 2), "12.34");
        assert_eq!(unpack(&[0x00, 0x5d], 0), "-5");
    }

    #[test]
    fn command_output_as_messages() {
        let m = messages("CPC2102 Library X created.\nplain\n", None);
        assert_eq!(m[0].id, "CPC2102");
        assert_eq!(m[0].text, "Library X created.");
        assert_eq!(m[1].id, "");
    }
}
