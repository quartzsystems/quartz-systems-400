// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::config::Config` as the user: system values from
//! `sysval_panel_list` and `sysval_panel` (`WRKSYSVAL`, `DSPSYSVAL`)
//! with what each accepts from the `lic/sysval` catalog; disks from
//! `panel2_data DSKSTS` (`WRKDSKSTS`) and `SYSSTS`'s ASP figures;
//! hardware from `hw_table` in `WRKHDWRSC`'s tree order (`view.rs`)
//! with `hw_cfgds` by resource; time from the date and time values,
//! `CHGNTPA`'s attributes (`tcp Servers`) and the `QTOTNTP` job.

use std::time::{Duration, Instant};

use hwrsrc::Kind;
use hwrsrc::view;
use navigator::env::config::{Allowed, Asp, Config, DiskUnit, Disks, Resource, SysVal, Time};
use qsweb::msg::Message;
use servers::attrs::ntp::NtpAttrs;
use stg::Client;
use stg::tcp::{TcpOp, TcpReply};
use sysval::{Catalog, Entry, Group, Kind as SvKind};

use super::Link;
use super::parse;
use super::perf::{panel, val};
use crate::work::netsvc::now_micros;
use crate::work::ntp::{SYNC_DONE, SYNC_REQ};
use crate::work::web::bridge::Main;

/// What `QTIMADJ` names while the SNTP client adjusts the clock.
const SNTP: &str = crate::work::ntp::ADJUSTER;

/// System values, disks, hardware and time.
pub struct LiveConfig {
    /// The job's thread.
    pub link: Link,
}

/// The Navigator category of a value: its catalog group, with the
/// values IBM's Navigator files elsewhere.
fn category(name: &str, group: Option<Group>) -> &'static str {
    match name {
        "QDSCJOBITV" => return "security",
        "QAUTOCFG" | "QAUTORMT" | "QAUTOVRT" | "QDEVNAMING" | "QDEVRCYACN" | "QPRTDEV"
        | "QCMNRCYLMT" | "QCMNARB" | "QCONSOLE" | "QSTRPRTWTR" => return "devices",
        "QPWRDWNLMT" | "QPWRRSTIPL" | "QUPSDLYTIM" | "QUPSMSGQ" | "QIPLDATTIM" | "QRMTIPL" => {
            return "power";
        }
        "QSAVACCPTH" => return "save",
        "QDYNPTYADJ" | "QDYNPTYSCD" | "QPFRADJ" | "QQRYDEGREE" | "QQRYTIMLMT" | "QTHDRSCADJ"
        | "QTHDRSCAFN" | "QMAXACTLVL" | "QBASACTLVL" | "QTSEPOOL" => return "perf",
        "QENDJOBLMT" | "QSETJOBATR" | "QALWJOBITP" => return "jobs",
        _ => {}
    }
    match group {
        Some(Group::Sec) => "security",
        Some(Group::Dattim | Group::Edt) => "datetime",
        Some(Group::Alc) => "jobs",
        Some(Group::Stg) => "storage",
        Some(Group::Msg) => "msgs",
        Some(Group::Libl) => "libl",
        Some(Group::Sysctl | Group::Net) | None => "sysctl",
    }
}

/// A catalog default as `DSPSYSVAL` would show it.
fn shipped(e: &Entry) -> String {
    e.default_value().text()
}

/// What a value accepts, as its catalog says.
fn allowed(e: &Entry) -> Allowed {
    match (e.kind, &e.allowed, e.range) {
        (SvKind::List, Some(v), _) => Allowed::List(v.clone(), e.elems.unwrap_or(v.len())),
        (_, Some(v), _) => Allowed::Values(v.clone()),
        (SvKind::Dec, None, Some([min, max])) => Allowed::Range(min, max, Vec::new()),
        _ => Allowed::Any,
    }
}

/// The catalog, parsed once.
fn catalog() -> Option<&'static Catalog> {
    static CAT: std::sync::OnceLock<Option<Catalog>> = std::sync::OnceLock::new();
    CAT.get_or_init(|| Catalog::builtin().ok()).as_ref()
}

fn sysvals(m: &Main) -> Vec<SysVal> {
    let cat = catalog();
    let all = m.ask(Client::sysval_panel_list);
    all.into_iter()
        .map(|(name, _, text)| {
            let n = name.clone();
            let value = m
                .ask(|l| l.sysval_panel(n))
                .map(|(_, _, elems)| elems.iter().map(|e| e.trim()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            let e = cat.and_then(|c| c.get(&name));
            SysVal {
                category: category(&name, e.map(|e| e.group)).to_string(),
                value: value.trim().to_string(),
                shipped: e.map(shipped).unwrap_or_default(),
                text,
                allowed: e.map(allowed).unwrap_or_default(),
                ipl: e.is_some_and(|e| e.restart_required),
                changed: None,
                name,
            }
        })
        .collect()
}

/// `QTIMZON` and the other values Time management shows.
fn sysval(m: &Main, name: &str) -> String {
    let n = name.to_string();
    m.ask(|l| l.sysval(n))
        .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
        .unwrap_or_default()
}

/// The units' resource names by serial number, from the hardware table.
pub fn disk_names(m: &Main) -> Vec<(String, String)> {
    m.ask(|l| l.hw_table(false))
        .map(|t| {
            t.resources
                .into_iter()
                .filter(|r| r.kind == Kind::Disk && !r.serial.is_empty())
                .map(|r| (r.serial, r.name))
                .collect()
        })
        .unwrap_or_default()
}

/// A unit's resource name: the disk with its serial number, else
/// `DDnnn` from the unit number.
pub fn unit_resource(names: &[(String, String)], serial: &str, unit: u32) -> String {
    names
        .iter()
        .find(|(s, _)| !serial.is_empty() && s == serial)
        .map_or_else(|| format!("DD{unit:03}"), |(_, n)| n.clone())
}

fn disks(m: &Main, reset: bool) -> Disks {
    let (vars, rows) = panel(m, "DSKSTS", &[if reset { "*YES" } else { "*NO" }]);
    let names = disk_names(m);
    let units: Vec<DiskUnit> = rows
        .iter()
        .map(|r| {
            let unit = val(r, "UNIT").trim().parse().unwrap_or(0);
            let serial = val(r, "SERIAL").trim().to_string();
            let bytes = val(r, "SIZEB").trim().parse::<u64>().ok();
            DiskUnit {
                unit,
                resource: unit_resource(&names, &serial, unit),
                kind: val(r, "TYPE").trim().to_string(),
                model: val(r, "MODEL").trim().to_string(),
                capacity: bytes.unwrap_or_else(|| parse::size(&format!("{} G", val(r, "SIZE")))),
                used_pct: parse::tenths(val(r, "USED")),
                protection: match val(r, "PROT").trim() {
                    "" => "None".into(),
                    p => p.to_string(),
                },
                status: match val(r, "STATUS").trim() {
                    "" => "Active".into(),
                    s => s.to_string(),
                },
                asp: Some(1),
                serial,
            }
        })
        .collect();
    let (sys, _) = panel(m, "SYSSTS", &["*NO"]);
    let capacity = parse::size(val(&sys, "SYSASP"));
    let pct = val(&sys, "ASPPCT").trim().parse::<f64>().unwrap_or(0.0);
    let used_pct = parse::tenths(val(&sys, "ASPPCT"));
    // From the exact percent: the tenths round a fresh system to 0.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let used = (capacity as f64 * pct / 100.0).round().max(0.0) as u64;
    let protection = asp_protection(&units);
    Disks {
        asps: vec![Asp {
            number: 1,
            kind: "System".into(),
            capacity,
            used,
            used_pct,
            protection,
            overflow: Some(false),
            threshold: 90,
        }],
        units,
        elapsed: parse::hms(val(&vars, "ELAPSED")),
    }
}

/// An ASP's protection from its units': `Mirrored`, `Device parity`,
/// both, or `None`. Parity names its level, `Device parity (RAID 5)`,
/// when every parity unit has the same one.
fn asp_protection(units: &[DiskUnit]) -> String {
    let has = |p: &str| units.iter().any(|u| u.protection.starts_with(p));
    let mut parity: Vec<&str> = units
        .iter()
        .filter(|u| u.protection.starts_with("Device parity"))
        .map(|u| u.protection.as_str())
        .collect();
    parity.dedup();
    let parity = match parity.as_slice() {
        [one] => (*one).to_string(),
        _ => "Device parity".into(),
    };
    match (has("Mirrored"), has("Device parity")) {
        (true, true) => format!(
            "Mirrored, device parity{}",
            parity.strip_prefix("Device parity").unwrap_or("")
        ),
        (true, false) => "Mirrored".into(),
        (false, true) => parity,
        (false, false) => "None".into(),
    }
}

/// Where a kind of resource is listed.
fn hw_category(k: Kind) -> &'static str {
    match k {
        Kind::Lan | Kind::Port | Kind::Comm | Kind::Channel => "Communications",
        Kind::Storage | Kind::Disk | Kind::Optical | Kind::Tape | Kind::TapeLibrary => "Storage",
        Kind::Processor | Kind::Memory => "Processor",
        Kind::Keyboard => "Workstation",
        Kind::Bridge | Kind::Other => "",
    }
}

/// `*LIND` as `VRYCFG`'s `CFGTYPE` (`*LIN`).
fn cfg_type(t: &str) -> String {
    match t {
        "*LIND" => "*LIN".into(),
        "*CTLD" => "*CTL".into(),
        "*DEVD" => "*DEV".into(),
        other => other.to_string(),
    }
}

fn resources(m: &Main) -> Vec<Resource> {
    let Ok(t) = m.ask(|l| l.hw_table(false)) else {
        return Vec::new();
    };
    let Some(all) = view::spec("*AHW") else {
        return Vec::new();
    };
    view::list(&t, all, &mut |id, v| super::db::text_of(m, id, v))
        .into_iter()
        .map(|row| {
            let r = t.get(&row.name);
            let n = row.name.clone();
            let cfg = m
                .ask(|l| l.hw_cfgds(n))
                .into_iter()
                .map(|(d, ty)| (d, cfg_type(&ty)))
                .collect();
            Resource {
                depth: u32::try_from(row.depth).unwrap_or(0),
                type_model: row.type_col.trim().to_string(),
                text: r.map_or_else(|| row.text.clone(), |r| r.description.clone()),
                status: row.status.clone(),
                serial: r.map(|r| r.serial.clone()).unwrap_or_default(),
                location: r.map(view::location).unwrap_or_default(),
                category: r.map_or("", |r| hw_category(r.kind)).to_string(),
                cfg,
                name: row.name,
            }
        })
        .collect()
}

/// `-04:00` from seconds east of UTC.
fn offset_text(secs: i32) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let a = secs.unsigned_abs();
    format!("{sign}{:02}:{:02}", a / 3600, a % 3600 / 60)
}

/// The `*NTP` server table bag.
fn ntp_bag(m: &Main) -> Vec<(String, String)> {
    match m.ask(|l| l.tcp(TcpOp::Servers)) {
        TcpReply::Servers { list } => list
            .into_iter()
            .find(|s| s.server == "*NTP")
            .map(|s| s.bag)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `QTOTNTP`'s last poll from the bag: `LASTSYNC` (`seconds
/// offset-µs`) and `LASTFAIL`.
fn last_poll(bag: &[(String, String)]) -> (Option<(u64, i64)>, Option<String>) {
    let get = |k: &str| bag.iter().find(|(x, _)| x == k).map(|(_, v)| v.trim());
    let sync = get("LASTSYNC").and_then(|v| {
        let (at, us) = v.split_once(' ')?;
        Some((at.parse().ok()?, us.trim().parse().ok()?))
    });
    let fail = get("LASTFAIL")
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    (sync, fail)
}

fn time(m: &Main) -> Time {
    let now = m.ask(Client::now);
    let zone = sysval(m, "QTIMZON");
    let bag = ntp_bag(m);
    let a = NtpAttrs::from_bag(&bag);
    let (last_sync, last_fail) = last_poll(&bag);
    let jobs = m.ask(Client::active_jobs);
    // The descriptions in QSYS as the service has them (CRTTIMZON's too).
    let (_, rows) = m
        .ask(|l| l.panel2_data("TIMZON".into(), vec!["*ALL".into()]))
        .unwrap_or_default();
    let cell = |r: &[(String, String)], k: &str| {
        r.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let mut zones: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            let text = cell(r, "TEXT");
            (
                cell(r, "TIMZON"),
                if text.trim().is_empty() {
                    cell(r, "FULL")
                } else {
                    text
                },
            )
        })
        .collect();
    if !zone.is_empty() && !zones.iter().any(|(n, _)| *n == zone) {
        zones.push((zone.clone(), String::new()));
    }
    let adjust = sysval(m, "QTIMADJ");
    let mut adjusters = vec![
        (
            "*NONE".to_string(),
            "No time adjustment software".to_string(),
        ),
        (SNTP.to_string(), "The system's SNTP client".to_string()),
    ];
    if !adjust.is_empty()
        && !adjusters
            .iter()
            .any(|(v, _)| v.eq_ignore_ascii_case(&adjust))
    {
        adjusters.push((adjust.clone(), String::new()));
    }
    Time {
        zone,
        zones,
        offset: offset_text(now.offset_min * 60),
        dst: now.dst,
        date_format: sysval(m, "QDATFMT"),
        date_sep: sysval(m, "QDATSEP"),
        time_sep: sysval(m, "QTIMSEP"),
        adjust,
        adjusters,
        ntp_servers: a.rmtsys,
        last_sync,
        last_fail,
        ntp_active: jobs.iter().any(|j| j.name == "QTOTNTP"),
    }
}

impl Config for LiveConfig {
    fn sysvals(&self, user: &str) -> Vec<SysVal> {
        self.link.read(user, |m| sysvals(m))
    }

    fn disks(&self, user: &str, reset: bool) -> Disks {
        self.link.read(user, move |m| disks(m, reset))
    }

    fn resources(&self, user: &str) -> Vec<Resource> {
        self.link.read(user, |m| resources(m))
    }

    fn time(&self, user: &str) -> Time {
        self.link.read(user, |m| time(m))
    }

    fn sync_now(&self, user: &str) -> bool {
        let token = now_micros().to_string();
        let set = token.clone();
        let asked = self.link.read(user, move |m| {
            let op = TcpOp::SetServer {
                server: "*NTP".into(),
                autostart: None,
                bag: vec![(SYNC_REQ.to_string(), set)],
            };
            !matches!(m.ask(|l| l.tcp(op)), TcpReply::Failed { .. })
        });
        if !asked {
            return false;
        }
        // The job looks every two seconds; a poll takes two per server.
        let until = Instant::now() + Duration::from_secs(10);
        while Instant::now() < until {
            std::thread::sleep(Duration::from_millis(500));
            let t = token.clone();
            let done = self.link.read(user, move |m| {
                ntp_bag(m).iter().any(|(k, v)| k == SYNC_DONE && *v == t)
            });
            if done {
                return true;
            }
        }
        false
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        let c = Catalog::builtin().unwrap();
        let of = |n: &str| category(n, c.get(n).map(|e| e.group));
        assert_eq!(of("QSECURITY"), "security");
        assert_eq!(of("QDATFMT"), "datetime");
        assert_eq!(of("QMAXJOB"), "jobs");
        assert_eq!(of("QDEVNAMING"), "devices");
        assert_eq!(of("QPFRADJ"), "perf");
        assert_eq!(of("QCTLSBSD"), "sysctl");
        assert_eq!(of("QSYSLIBL"), "libl");
        assert_eq!(of("QHSTLOGSIZ"), "msgs");
        assert_eq!(of("QSTGLOWLMT"), "storage");
        assert_eq!(of("QUPSDLYTIM"), "power");
        assert_eq!(of("QSAVACCPTH"), "save");
        let v = c.get("QALWOBJRST").unwrap();
        assert_eq!(shipped(v), "*ALL");
        assert_eq!(offset_text(-4 * 3600), "-04:00");
        assert_eq!(offset_text(5 * 3600 + 1800), "+05:30");
        let bag = |s: &str, f: &str| {
            vec![
                ("LASTSYNC".to_string(), s.to_string()),
                ("LASTFAIL".to_string(), f.to_string()),
            ]
        };
        assert_eq!(
            last_poll(&bag("1793000000 -4000", "")),
            (Some((1_793_000_000, -4000)), None)
        );
        assert_eq!(
            last_poll(&bag("", "h: timed out")),
            (None, Some("h: timed out".into()))
        );
    }
}
