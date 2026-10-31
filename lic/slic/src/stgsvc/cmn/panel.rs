// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `WRKCFGSTS`, `WRKLIND`, `DSPLIND` and `RTVCFGSTS` show: the
//! lines' status from the shared state, merged with the controllers
//! and devices that are not a line's (`QPACTL01`, displays, printers),
//! as panel data for the job and as the service's text.

use std::fmt::Write as _;

use cmncfg::{Kind, Lind, Status};
use stg::proto::Sys2Data;

use super::{kernel, lock, objects};
use crate::ipl::system::System;

/// One row of `WRKCFGSTS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CfgRow {
    /// The object.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Levels under the object asked for.
    pub depth: u8,
    /// The status text.
    pub status: String,
    /// The job using it: name, user, number; blank for none.
    pub job: Option<(String, String, u32)>,
}

/// Whether `name` of class `class` (`*ELAN`, `*NET`, `*VWS`, `*DSP`,
/// `*PRT`) passes `CFGD`: `*ALL`, `*CMN`, a class, a generic name or a
/// name.
fn wanted(cfgd: &str, name: &str, class: &str) -> bool {
    match cfgd {
        "" | "*ALL" | "*CMN" => true,
        c if c.starts_with('*') => c == class,
        c => match c.strip_suffix('*') {
            Some(p) => name.starts_with(p),
            None => name == c,
        },
    }
}

/// The status a stored object's text stands for, for `STATUS(...)`.
fn as_status(text: &str) -> Status {
    match text {
        "VARIED OFF" | "FAILED" => Status::VariedOff,
        "ACTIVE" | "UNAVAILABLE" => Status::Active,
        "RCYPND" => Status::RcyPnd,
        _ => Status::VariedOn,
    }
}

/// A device that is not a line's: its status text.
fn dev_text(d: &wm::obj::devd::Devd) -> &'static str {
    if d.is_display() || d.media.is_some() {
        d.status.text()
    } else if d.online {
        "VARIED ON"
    } else {
        "VARIED OFF"
    }
}

/// A display's class for `CFGD`.
fn dev_class(d: &wm::obj::devd::Devd) -> &'static str {
    if let Some(k) = d.media_kind() {
        k.devcls()
    } else if d.is_network() {
        "*NET"
    } else if d.is_printer() {
        "*PRT"
    } else {
        "*DSP"
    }
}

/// The interactive job at display `dev`, when one is signed on.
fn job_at(system: &System, dev: &str) -> Option<(String, String, u32)> {
    system
        .jobs
        .all()
        .into_iter()
        .find(|j| j.name == dev && j.kind == 'I')
        .map(|j| (j.name, j.user, j.number))
}

/// The rows of `WRKCFGSTS CFGTYPE(kind) CFGD(cfgd) RANGE(...)
/// STATUS(filter)`: each object asked for with what hangs off it.
pub(crate) fn cfgsts_rows(
    system: &mut System,
    kind: Kind,
    cfgd: &str,
    range_net: bool,
    filter: &str,
) -> Result<Vec<CfgRow>, String> {
    let st = |r: &cmncfg::Row| CfgRow {
        name: r.name.clone(),
        kind: r.kind,
        depth: r.depth,
        status: r.status.text().to_string(),
        job: None,
    };
    // Groups: the object asked for, then what hangs off it.
    let mut groups: Vec<Vec<CfgRow>> = Vec::new();
    let push_rows = |groups: &mut Vec<Vec<CfgRow>>, rows: Vec<cmncfg::Row>| {
        for r in &rows {
            if r.depth == 0 {
                groups.push(Vec::new());
            }
            if let Some(g) = groups.last_mut() {
                g.push(st(r));
            }
        }
    };
    match kind {
        Kind::Line => {
            let rows = lock()
                .state
                .rows(Kind::Line, &|n| wanted(cfgd, n, "*ELAN"), range_net);
            push_rows(&mut groups, rows);
        }
        Kind::Ctl => {
            let rows = lock()
                .state
                .rows(Kind::Ctl, &|n| wanted(cfgd, n, "*NET"), range_net);
            push_rows(&mut groups, rows);
            let devs = objects::devds(system)?;
            for (n, c) in objects::ctlds(system)? {
                if !c.line.is_empty() || !wanted(cfgd, &n, &c.ctltype) {
                    continue;
                }
                let kids: Vec<CfgRow> = devs
                    .iter()
                    .filter(|(_, d)| d.ctl == n && !d.is_network())
                    .map(|(dn, d)| CfgRow {
                        name: dn.clone(),
                        kind: Kind::Dev,
                        depth: 1,
                        status: dev_text(d).into(),
                        job: (d.status == wm::obj::devd::DevStatus::Active)
                            .then(|| job_at(system, dn))
                            .flatten(),
                    })
                    .collect();
                let on_kid = kids.iter().any(|k| k.status != "VARIED OFF");
                let status = match (c.online, on_kid) {
                    (false, _) => "VARIED OFF",
                    (true, true) => "ACTIVE",
                    (true, false) => "VARIED ON",
                };
                let mut g = vec![CfgRow {
                    name: n.clone(),
                    kind: Kind::Ctl,
                    depth: 0,
                    status: status.into(),
                    job: None,
                }];
                if range_net {
                    g.extend(kids);
                }
                groups.push(g);
            }
        }
        Kind::Dev => {
            let rows = lock()
                .state
                .rows(Kind::Dev, &|n| wanted(cfgd, n, "*NET"), range_net);
            push_rows(&mut groups, rows);
            for (n, d) in objects::devds(system)? {
                if d.is_network() || !wanted(cfgd, &n, dev_class(&d)) {
                    continue;
                }
                groups.push(vec![CfgRow {
                    job: (d.status == wm::obj::devd::DevStatus::Active)
                        .then(|| job_at(system, &n))
                        .flatten(),
                    name: n,
                    kind: Kind::Dev,
                    depth: 0,
                    status: dev_text(&d).into(),
                }]);
            }
        }
    }
    groups.retain(|g| {
        g.first()
            .is_some_and(|r| as_status(&r.status).passes(filter))
    });
    groups.sort_by(|a, b| a[0].name.cmp(&b[0].name));
    Ok(groups.into_iter().flatten().collect())
}

/// `(variable, value)` pairs.
fn vars(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

/// The panel data the job asks for (`stg` request 1180).
pub(crate) fn panel(system: &mut System, what: &str, args: &[String]) -> Result<Sys2Data, String> {
    let arg = |i: usize| {
        args.get(i)
            .map_or(String::new(), |s| s.trim().to_ascii_uppercase())
    };
    match what {
        "CFGSTS" => {
            let kind = Kind::parse(&arg(0))
                .ok_or_else(|| format!("CPD0084 CFGTYPE({}) is not *LIN, *CTL or *DEV", arg(0)))?;
            let rows = cfgsts_rows(system, kind, &arg(1), arg(2) != "*OBJ", &arg(3))?;
            let list = rows
                .into_iter()
                .map(|r| {
                    vars(&[
                        (
                            "CFNAME",
                            format!("{}{}", "  ".repeat(usize::from(r.depth)), r.name),
                        ),
                        ("CFOBJ", r.name),
                        ("CFSTS", r.status),
                        (
                            "CFJOB",
                            r.job
                                .map(|(j, u, n)| format!("{j:<12}{u:<12}{n:06}"))
                                .unwrap_or_default(),
                        ),
                        ("CFTYPE", r.kind.text().into()),
                    ])
                })
                .collect();
            Ok((Vec::new(), vec![("CFGS".into(), list)]))
        }
        "LIND" => {
            let which = arg(0);
            let lines = objects::lines(system)?;
            let sh = lock();
            let list = lines
                .into_iter()
                .filter(|(n, _)| wanted(&which, n, "*ELAN"))
                .map(|(n, l)| {
                    let status = sh.state.line(&n).map_or(Status::VariedOff, |s| s.status);
                    vars(&[
                        ("LIND", n),
                        ("LINTYPE", "*ELAN".into()),
                        ("LINTEXT", l.text.clone()),
                        ("LINRSC", l.rsrcname.clone()),
                        ("LINSTS", status.text().into()),
                    ])
                })
                .collect();
            Ok((Vec::new(), vec![("LINES".into(), list)]))
        }
        "DSPLIND" => {
            let name = arg(0);
            let l = objects::read_lind(system, &name)?;
            Ok((dsplind_vars(system, &name, &l), Vec::new()))
        }
        other => Err(format!("CPF9898 no communications display {other}")),
    }
}

/// The panel variable of each `DSPLIND` label.
const DSPLIND_VARS: &[(&str, &str)] = &[
    ("Resource name", "DRSRC"),
    ("Aggregate policy", "DAGGPCY"),
    ("Aggregated resource list", "DAGGRSCL"),
    ("Bridge identifier", "DBRIDGE"),
    ("Online at IPL", "DONLINE"),
    ("Vary on wait", "DVRYWAIT"),
    ("Network controller", "DNETCTL"),
    ("Local adapter address", "DADPTADR"),
    ("Exchange identifier", "DEXCHID"),
    ("Ethernet standard", "DETHSTD"),
    ("Line speed", "DSPEED"),
    ("Current line speed", "DCURSPD"),
    ("Duplex", "DDUPLEX"),
    ("Current duplex", "DCURDPX"),
    ("Serviceability options", "DSRVOPT"),
    ("Maximum frame size", "DMAXFRM"),
    ("Maximum controllers", "DMAXCTL"),
    ("Error threshold level", "DTHRESH"),
    ("Generate test frame", "DGENTST"),
    ("Message queue", "DMSGQ"),
    ("Current message queue", "DCURMSGQ"),
    ("  Library", "DCURMSGL"),
    ("Text", "DTEXT"),
];

/// `DSPLIND`'s rows with what only the running system knows: the
/// controller, the port's address for `*ADPT`, the current speed and
/// duplex while the line is varied on.
pub(crate) fn dsplind_rows(
    system: &mut System,
    name: &str,
    l: &Lind,
) -> Vec<(String, String, String)> {
    let (_, cfgmsgq) = super::defaults(system);
    let msgq = l.resolved_msgq(&cfgmsgq);
    let (ctl, ifname, on) = {
        let sh = lock();
        sh.state
            .line(name)
            .map_or((String::new(), String::new(), false), |s| {
                (s.ctl_name(), s.ifname.clone(), s.status.is_on())
            })
    };
    let mac = (l.adptadr == "*ADPT")
        .then(|| {
            system
                .hw()
                .resources
                .iter()
                .find(|r| r.name == l.rsrcname)
                .and_then(|r| r.port.as_ref())
                .map(|p| p.mac.replace(':', "").to_ascii_uppercase())
        })
        .flatten();
    let mut rows = l.display_rows(&ctl, &msgq);
    for (_, label, value) in &mut rows {
        match label.as_str() {
            "Local adapter address" => {
                if let Some(m) = &mac {
                    value.clone_from(m);
                }
            }
            "Current line speed" if on => {
                if let Some(s) = kernel::current_speed(&ifname) {
                    *value = s;
                }
            }
            "Current duplex" if on => {
                if let Some(s) = kernel::current_duplex(&ifname) {
                    *value = s;
                }
            }
            _ => {}
        }
    }
    rows
}

fn dsplind_vars(system: &mut System, name: &str, l: &Lind) -> Vec<(String, String)> {
    let kv = |k: &str, v: &str| format!("{k:<12}{v}");
    let mut out = vec![
        ("DLIND".to_string(), kv("LIND", name)),
        ("DOPTION".to_string(), kv("OPTION", "*BASIC")),
        ("DCAT".to_string(), kv("", "*ELAN")),
        (
            "AGG".to_string(),
            if l.is_aggregate() { "1" } else { "0" }.to_string(),
        ),
    ];
    for (kwd, label, value) in dsplind_rows(system, name, l) {
        if let Some((_, var)) = DSPLIND_VARS.iter().find(|(lb, _)| *lb == label) {
            out.push(((*var).to_string(), kv(&kwd, &value)));
        }
    }
    out
}

/// `RTVCFGSTS`: a line's, controller's or device's status code and
/// text (`stg` request 1181).
pub(crate) fn cfg_status(system: &mut System, name: &str, ty: &str) -> Option<(u16, String)> {
    let kind = Kind::parse(&ty.to_ascii_uppercase())?;
    let name = name.to_ascii_uppercase();
    if let Some(s) = lock().state.status_of(kind, &name) {
        return Some((s.code(), s.text().into()));
    }
    let rows = cfgsts_rows(system, kind, &name, false, "*ALL").ok()?;
    let r = rows.into_iter().find(|r| r.name == name)?;
    let code = match r.status.as_str() {
        "VARIED OFF" => 0,
        "SIGNON DISPLAY" => 50,
        "ACTIVE" => 60,
        "RCYPND" => 80,
        _ => 30,
    };
    Some((code, r.status))
}

/// `WRKCFGSTS` as text, for a job without a display.
pub(crate) fn cfgsts_text(rows: &[CfgRow]) -> String {
    let mut out = String::from("Description         Status                Job\n");
    for r in rows {
        let name = format!("{}{}", "  ".repeat(usize::from(r.depth)), r.name);
        let job = r
            .job
            .as_ref()
            .map(|(j, u, n)| format!("{n:06}/{u}/{j}"))
            .unwrap_or_default();
        let _ = writeln!(out, "{name:<18}  {:<20}  {job}", r.status);
    }
    out
}

/// `DSPLIND` as text.
pub(crate) fn dsplind_text(system: &mut System, name: &str, l: &Lind) -> String {
    let mut out = format!("Line description . . . . : {name}\nCategory of line . . . . : *ELAN\n");
    for (kwd, label, value) in dsplind_rows(system, name, l) {
        let _ = writeln!(out, "{label:<28} {kwd:<11} {value}");
    }
    out
}
