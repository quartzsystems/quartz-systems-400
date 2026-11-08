// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Manage` service (version 7) on a connection job's secondary
//! thread, over the thread's link, as the signed-on profile. Each
//! operation does what its command does: `STRSBS`, `ENDSBS`,
//! `CHGSYSVAL` and `PWRDWNSYS` go through the analyzer (the `*CMD`'s
//! authority), the service-site ones to their processing programs
//! (`run_cmd`), `STRSBS` starts the monitor as the job's command does
//! after the service's check (`*JOBCTL`), `PWRDWNSYS` through the
//! service's check (`stgsvc/qap/manage.rs`). The lists read what the
//! displays read: `sbs_list` and the `*SBSD` objects, the system value
//! panels' data, the hardware resource table in `WRKHDWRSC`'s order;
//! PTFs, disks and the IPL from the service. `QS400_QAP_MGMT`'s exit
//! programs are called in a runtime job of the thread's own, built the
//! first time a program is registered.

use std::sync::{Mutex, PoisonError};

use cmd::Resolved;
use cmd::analyze::Env;
use cmd::def::Mode;
use mi::Job;
use qap_proto::Status;
use qap_proto::ops::manage::{
    EndHow, EndSubsystem, HardwareResource, HardwareResources, PowerDown, ResourceStatus,
    ResourceType, SetSystemValue, Subsystem, SubsystemStatus, SystemValue, SystemValues, ValueKind,
};
use qap_proto::types::{JobId, ObjectName};
use qapd::Refusal;
use qapd::manage::ExitRequest;
use regfac::ProgramCaller;
use stg::proto::CmdReply;
use stg::remote::{RemoteCmds, RemoteRegistry, Sources};
use stg::{Client, Link, Shared};
use sysapi::ObjInfo;

use crate::work::spawn::Spawned;

/// Monitors this job's threads started, until they end.
static MONITORS: Mutex<Vec<Spawned>> = Mutex::new(Vec::new());

/// `TCP1A5E`: an exit program refused.
const EXIT_REFUSED: &str = qapd::manage::EXIT_REFUSED;
/// `TCP1A5F`: a value a system value does not take.
const BAD_VALUE: &str = "TCP1A5F";

/// A thread's reach: its link, as the signed-on profile.
pub(super) struct Ctx<'a> {
    pub shared: &'a Shared,
}

impl Ctx<'_> {
    fn link(&self) -> std::cell::RefMut<'_, Link> {
        self.shared.borrow_mut()
    }

    fn refuse(&self, status: Status, id: &str, data: &[&str]) -> Refusal {
        let message = super::host::message(&mut self.link(), id, data);
        Refusal { status, message }
    }

    /// A message with its data laid out and the service's text, when it
    /// gave one.
    fn refuse_data(&self, status: Status, id: &str, data: &[u8], text: &str) -> Refusal {
        let mut message = super::host::message_bytes(&mut self.link(), id, data);
        if !text.trim().is_empty() {
            message.text = text.trim().to_string();
            message.help.clear();
        }
        Refusal { status, message }
    }

    /// A `CPFnnnn text` error: the status by its identifier, the
    /// service's text.
    fn refuse_text(&self, e: &str) -> Refusal {
        let id = e
            .split_whitespace()
            .next()
            .filter(|w| w.len() == 7 && w.chars().all(|c| c.is_ascii_alphanumeric()))
            .unwrap_or("CPF9898");
        let text = e.strip_prefix(id).unwrap_or(e);
        self.refuse_data(status_of(id), id, &[], text)
    }
}

/// The status a command's message stands for.
fn status_of(id: &str) -> Status {
    match id {
        "CPF222E" | "CPF18B4" | "CPF9802" | "CPF2189" | "CPF2182" | "CPF2207" | "CPD0032"
        | "CPD0171" | "MCH1001" => Status::NotAuthorized,
        "CPF9801" | "CPF1003" | "CPF2110" | "CPF1842" | "CPD0030" => Status::NotFound,
        _ => Status::Error,
    }
}

/// Analyze `line` as a command typed in the job: the `*CMD` found
/// through the library list, its authority checked.
pub(super) fn analyze(cx: &Ctx<'_>, line: &str) -> Result<Resolved, Refusal> {
    let mut cmds = RemoteCmds::new(cx.shared.clone());
    let r = {
        let mut env = Env::new(&mut cmds);
        env.mode = Mode::PROD;
        cmd::analyze(&mut env, line)
    };
    r.map_err(|e| {
        let d = e.diags.first().unwrap_or(&e.escape).clone();
        cx.refuse_data(status_of(&d.msgid), &d.msgid, &d.data, &d.detail)
    })
}

/// The read operations need what their displays need: the command,
/// found through the analyzer's own source as a typed line finds it,
/// with the profile's `*OBJOPR` and `*USE` to it (`DSPPTF` is shipped
/// `*PUBLIC *EXCLUDE`). A command not there is `CPD0030`, never a
/// pass.
pub(super) fn may_use(cx: &Ctx<'_>, command: &str) -> Result<(), Refusal> {
    let mut cmds = RemoteCmds::new(cx.shared.clone());
    match cmd::analyze::CmdSource::lookup(&mut cmds, command, None) {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(cx.refuse(Status::NotFound, "CPD0030", &[command, "*LIBL"])),
        Err(e) => {
            let d = e.diags.first().unwrap_or(&e.escape).clone();
            Err(cx.refuse_data(Status::NotAuthorized, &d.msgid, &d.data, &d.detail))
        }
    }
}

/// Run a service-site command as `crate::command::service` does.
pub(super) fn service(cx: &Ctx<'_>, r: &Resolved) -> Result<CmdReply, Refusal> {
    let built = cmd::block::build(&r.def, &r.parms)
        .map_err(|e| cx.refuse_text(&format!("{} {e}", e.msgid())))?;
    let specified: Vec<bool> = r.parms.iter().map(|b| b.specified).collect();
    let typed = sys_cpps::entry(&r.name)
        .map(|e| sys_cpps::bridge::specified_line(&e.word, &crate::command::parms_of(r)))
        .unwrap_or_default();
    let reply: CmdReply = cx
        .link()
        .with(|l| l.run_cmd(r.name.clone(), built.segments, specified, typed));
    match &reply.escape {
        Some(e) => {
            let first = reply.text.lines().next().unwrap_or("").trim();
            let text = first
                .strip_prefix(e.msgid.as_str())
                .unwrap_or(first)
                .to_string();
            Err(cx.refuse_data(status_of(&e.msgid), &e.msgid, &e.data, &text))
        }
        None => Ok(reply),
    }
}

/// The exit programs' runtime job, made at the first call.
struct ExitJob<'a> {
    shared: &'a Shared,
    job: Option<Box<Job>>,
}

impl ExitJob<'_> {
    fn job(&mut self) -> &mut Job {
        self.job.get_or_insert_with(|| {
            let s = Sources::new(self.shared);
            let mut job = Job::boxed(Box::new(s.objects));
            job.usrspcs.set_source(Box::new(s.spaces));
            job.msgs = Box::new(s.msgs);
            job.data = Some(Box::new(s.data));
            job.io = Some(Box::new(s.io));
            let provider: std::rc::Rc<std::cell::RefCell<dyn sysapi::Provider>> =
                std::rc::Rc::new(std::cell::RefCell::new(s.provider));
            sysapi::install(&mut job, provider);
            qrpgrt::install(&mut job);
            let me = self.shared.borrow_mut().with(Client::whoami);
            job.ident.user.clone_from(&me.user);
            job.act.qsecurity = me.qsecurity;
            let a = self.shared.borrow_mut().with(Client::rtvjoba);
            job.ident.libl = a.libl.search();
            job.ident.curlib = a.libl.cur;
            job
        })
    }
}

impl ProgramCaller for ExitJob<'_> {
    fn call(&mut self, pgm: &str, lib: &str, parms: &mut [Vec<u8>]) -> Result<(), String> {
        ProgramCaller::call(self.job(), pgm, lib, parms)
    }
}

/// `QS400_QAP_MGMT`: every registered program allows `req`, or none is
/// registered. A refusal is audited `GR` by the registry.
pub(super) fn exit_allows(
    cx: &Ctx<'_>,
    point: &str,
    format: &str,
    req: &ExitRequest,
) -> Result<(), Refusal> {
    let mut reg = RemoteRegistry::new(cx.shared.clone());
    let mut caller = ExitJob {
        shared: cx.shared,
        job: None,
    };
    let r = regfac::call_exit(
        &mut reg,
        &mut caller,
        point,
        &regfac::ExitRequest::host(format, req.format()),
    );
    if r.allow {
        return Ok(());
    }
    let (pgm, lib) = r.called.last().map_or_else(
        || ("*N".to_string(), "*N".to_string()),
        |c| (c.pgm.clone(), c.lib.clone()),
    );
    Err(cx.refuse(Status::NotAuthorized, EXIT_REFUSED, &[&pgm, &lib]))
}

fn object(lib: &str, name: &str) -> ObjectName {
    ObjectName {
        library: lib.to_string(),
        name: name.to_string(),
    }
}

/// `ListSubsystems`: the active subsystems, then the descriptions of
/// `library` (`*ALL`, `*LIBL`) not active, by name.
pub(super) fn subsystems(cx: &Ctx<'_>, library: &str) -> Result<Vec<Subsystem>, Refusal> {
    let lib = library.trim().to_ascii_uppercase();
    let descs: Vec<ObjInfo> = match lib.as_str() {
        "*ALL" => {
            let libs = cx
                .link()
                .with(|l| l.list_objects("QSYS".into(), "*ALL".into(), "*LIB".into()));
            let mut all = cx
                .link()
                .with(|l| l.list_objects("QSYS".into(), "*ALL".into(), "*SBSD".into()));
            for l in libs.iter().filter(|l| l.name != "QSYS") {
                all.extend(
                    cx.link()
                        .with(|k| k.list_objects(l.name.clone(), "*ALL".into(), "*SBSD".into())),
                );
            }
            all
        }
        "*LIBL" | "" => cx
            .link()
            .with(|l| l.list_objects("*LIBL".into(), "*ALL".into(), "*SBSD".into())),
        l => {
            let found = cx
                .link()
                .with(|k| k.object("QSYS".into(), l.into(), "*LIB".into()));
            if found.is_none() {
                return Err(cx.refuse(Status::NotFound, "CPF2110", &[l]));
            }
            cx.link()
                .with(|k| k.list_objects(l.into(), "*ALL".into(), "*SBSD".into()))
        }
    };
    let active = cx.link().with(Client::sbs_list);
    let text_of = |lib: &str, name: &str| {
        descs
            .iter()
            .find(|d| d.lib == lib && d.name == name)
            .map(|d| d.text.trim().to_string())
    };
    let mut out: Vec<Subsystem> = active
        .iter()
        .filter(|a| {
            matches!(lib.as_str(), "*ALL" | "*LIBL" | "")
                || a.lib == lib
                || text_of(&a.lib, &a.name).is_some()
        })
        .map(|a| Subsystem {
            sbsd: object(&a.lib, &a.name),
            status: if a.status.eq_ignore_ascii_case("ENDING") {
                SubsystemStatus::Ending
            } else {
                SubsystemStatus::Active
            },
            jobs: a.jobs,
            monitor: Some(JobId {
                name: a.name.clone(),
                user: "QSYS".into(),
                number: format!("{:06}", a.monitor),
            }),
            text: text_of(&a.lib, &a.name).unwrap_or_default(),
        })
        .collect();
    out.sort_by(|a, b| a.sbsd.name.cmp(&b.sbsd.name));
    let mut inactive: Vec<Subsystem> = descs
        .iter()
        .filter(|d| !active.iter().any(|a| a.name == d.name))
        .map(|d| Subsystem {
            sbsd: object(&d.lib, &d.name),
            status: SubsystemStatus::Inactive,
            text: d.text.trim().to_string(),
            ..Subsystem::default()
        })
        .collect();
    inactive.sort_by(|a, b| (&a.sbsd.name, &a.sbsd.library).cmp(&(&b.sbsd.name, &b.sbsd.library)));
    inactive.dedup_by(|a, b| a.sbsd.name == b.sbsd.name);
    out.extend(inactive);
    Ok(out)
}

/// `LIB/NAME` of a description, `*LIBL` when no library.
fn qualified(o: &ObjectName) -> String {
    let lib = o.library.trim();
    if lib.is_empty() {
        o.name.trim().to_ascii_uppercase()
    } else {
        format!(
            "{}/{}",
            lib.to_ascii_uppercase(),
            o.name.trim().to_ascii_uppercase()
        )
    }
}

/// `STRSBS`: the service checks (`*JOBCTL`, active), then the monitor
/// starts as the job's `STRSBS` starts it, and registers.
pub(super) fn start_subsystem(cx: &Ctx<'_>, sbsd: &ObjectName) -> Result<(), Refusal> {
    let qual = qualified(sbsd);
    analyze(cx, &format!("STRSBS SBSD({qual})"))?;
    let name = sbsd.name.trim().to_ascii_uppercase();
    // Each answer is taken before a refusal looks up its message.
    let checked = cx.link().with(|l| l.sbs_check(qual.clone()));
    let (lib, _) = checked.map_err(|e| cx.refuse_text(&e))?;
    let number = cx.link().with(Client::job_number);
    let number = number.map_err(|e| cx.refuse_text(&e))?;
    let start = wm::ctl::Start {
        name: name.clone(),
        user: "QSYS".into(),
        number,
        handle: u64::from(number),
        kind: 'M',
        subsystem: name.clone(),
        role: "MONITOR".into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        rqsdta: format!("{lib}/{name}"),
        pool: "*BASE".into(),
        ..wm::ctl::Start::default()
    };
    let mut s = crate::work::spawn::job(start).map_err(|e| cx.refuse_text(&e))?;
    crate::work::spawn::ready(&mut s).map_err(|e| cx.refuse_text(&e))?;
    {
        let mut m = MONITORS.lock().unwrap_or_else(PoisonError::into_inner);
        m.retain_mut(|s| matches!(s.child.try_wait(), Ok(None)));
        m.push(s);
    }
    for _ in 0..50 {
        let active = cx.link().with(Client::sbs_list);
        if active.iter().any(|a| a.name == name) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Err(cx.refuse_text(&format!("CPF1012 Subsystem {name} did not start.")))
}

fn how(h: EndHow) -> &'static str {
    match h {
        EndHow::Controlled => "*CNTRLD",
        EndHow::Immediate => "*IMMED",
    }
}

fn delay(d: Option<u32>) -> String {
    d.map_or_else(|| "*NOLIMIT".to_string(), |d| d.to_string())
}

/// `ENDSBS`, through its processing program.
pub(super) fn end_subsystem(cx: &Ctx<'_>, req: &EndSubsystem) -> Result<(), Refusal> {
    let sbs = req.sbsd.as_ref().map_or_else(
        || "*ALL".to_string(),
        |o| o.name.trim().to_ascii_uppercase(),
    );
    let r = analyze(
        cx,
        &format!(
            "ENDSBS SBS({sbs}) OPTION({}) DELAY({})",
            how(req.how),
            delay(req.delay)
        ),
    )?;
    service(cx, &r).map(|_| ())
}

fn kind_of(e: Option<&sysval::Entry>) -> ValueKind {
    match e.map(|e| e.kind) {
        Some(sysval::Kind::Dec) => ValueKind::Decimal,
        Some(sysval::Kind::List) => ValueKind::List,
        _ => ValueKind::Character,
    }
}

fn catalog() -> Option<sysval::Catalog> {
    sysval::Catalog::builtin().ok()
}

/// One value as `DSPSYSVAL` shows it; `None` when there is none.
fn value(link: &mut Link, cat: Option<&sysval::Catalog>, name: &str) -> Option<SystemValue> {
    let (text, group, elems) = link.with(|l| l.sysval_panel(name.to_string()))?;
    let entry = cat.and_then(|c| c.get(name));
    let value = elems
        .iter()
        .map(|e| e.trim_end())
        .filter(|e| !e.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(SystemValue {
        name: name.to_string(),
        kind: kind_of(entry),
        value,
        text,
        category: group.trim_start_matches('*').to_string(),
    })
}

/// `GetSystemValues`: in the order asked, every one by name when none
/// is named; the first unknown name is `CPF1842`.
pub(super) fn system_values(cx: &Ctx<'_>, names: &[String]) -> Result<SystemValues, Refusal> {
    let cat = catalog();
    let names: Vec<String> = if names.is_empty() {
        cx.link()
            .with(Client::sysval_panel_list)
            .into_iter()
            .map(|(n, _, _)| n)
            .collect()
    } else {
        names
            .iter()
            .map(|n| n.trim().to_ascii_uppercase())
            .collect()
    };
    let mut values = Vec::with_capacity(names.len());
    for n in &names {
        let v = value(&mut cx.link(), cat.as_ref(), n);
        match v {
            Some(v) => values.push(v),
            None => return Err(cx.refuse(Status::NotFound, "CPF1842", &[n])),
        }
    }
    Ok(SystemValues { values })
}

/// Whether two values say the same: blanks between words, case, and
/// leading zeros of a number aside.
fn same(kind: ValueKind, a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.split_whitespace()
            .map(str::to_ascii_uppercase)
            .collect::<Vec<_>>()
    };
    if kind == ValueKind::Decimal
        && let (Ok(x), Ok(y)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>())
    {
        return (x - y).abs() < f64::EPSILON;
    }
    norm(a) == norm(b)
}

/// `CHGSYSVAL`, through its processing program; the value as the
/// system now keeps it.
pub(super) fn set_system_value(cx: &Ctx<'_>, req: &SetSystemValue) -> Result<SystemValue, Refusal> {
    let name = req.name.trim().to_ascii_uppercase();
    let cat = catalog();
    let entry = cat.as_ref().and_then(|c| c.get(&name));
    let Some(entry) = entry else {
        return Err(cx.refuse(Status::NotFound, "CPF1842", &[&name]));
    };
    let bad = |cx: &Ctx<'_>| cx.refuse(Status::Error, BAD_VALUE, &[&name, &req.value]);
    let fits = !entry.readonly
        && entry
            .parse_value(&req.value)
            .is_ok_and(|v| entry.validate(&v).is_ok());
    if !fits {
        return Err(bad(cx));
    }
    let kind = kind_of(Some(entry));
    let quoted = req.value.replace('\'', "''");
    let r = analyze(cx, &format!("CHGSYSVAL SYSVAL({name}) VALUE('{quoted}')"))?;
    service(cx, &r)?;
    let now = value(&mut cx.link(), cat.as_ref(), &name);
    match now {
        Some(v) if same(kind, &v.value, &req.value) => Ok(v),
        _ => Err(bad(cx)),
    }
}

/// `ListHardwareResources`: what `WRKHDWRSC TYPE()` lists, in its order.
pub(super) fn hardware_resources(
    cx: &Ctx<'_>,
    kind: ResourceType,
) -> Result<HardwareResources, Refusal> {
    let table = cx.link().with(|l| l.hw_table(false));
    let table = table.map_err(|e| cx.refuse_text(&e))?;
    let Some(spec) = hwrsrc::view::spec(kind.special()) else {
        return Ok(HardwareResources::default());
    };
    let mut parents: Vec<String> = Vec::new();
    // The lists' words in the job's language, from QSTXTMSG.
    let mut words = |id: &str, v: &[&str]| super::host::message(&mut cx.link(), id, v).text;
    let resources = hwrsrc::view::list(&table, spec, &mut words)
        .into_iter()
        .map(|row| {
            parents.truncate(row.depth);
            let parent = parents.last().cloned().unwrap_or_default();
            parents.push(row.name.clone());
            let r = table.get(&row.name);
            HardwareResource {
                depth: u8::try_from(row.depth).unwrap_or(u8::MAX),
                type_model: r
                    .map_or_else(|| row.type_col.trim().to_string(), hwrsrc::view::type_model),
                status: if row.status == "Operational" {
                    ResourceStatus::Operational
                } else {
                    ResourceStatus::NotDetected
                },
                text: row.text.clone(),
                location: r.map(hwrsrc::view::location).unwrap_or_default(),
                serial: r.map(|r| r.serial.trim().to_string()).unwrap_or_default(),
                parent,
                name: row.name,
            }
        })
        .collect();
    Ok(HardwareResources { resources })
}

/// `PWRDWNSYS`: the command's authority; the service then checks
/// `*JOBCTL` and powers down (`qap_mgmt_power`).
pub(super) fn power_down_allowed(cx: &Ctx<'_>, req: &PowerDown) -> Result<(), Refusal> {
    let restart = if req.restart { "*YES" } else { "*NO" };
    let src = match req.ipl_source {
        qap_proto::ops::manage::IplSource::Panel => "*PANEL",
        qap_proto::ops::manage::IplSource::A => "A",
        qap_proto::ops::manage::IplSource::B => "B",
    };
    analyze(
        cx,
        &format!(
            "PWRDWNSYS OPTION({}) DELAY({}) RESTART({restart}) IPLSRC({src})",
            how(req.how),
            delay(req.delay)
        ),
    )
    .map(|_| ())
}
