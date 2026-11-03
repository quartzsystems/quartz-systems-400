// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/time`: the time zone, date and time formats, time adjustment and
//! the NTP client. Apply runs `CHGSYSVAL` for each value changed and
//! `CHGNTPA RMTSYS()` for the servers.

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::partials::{Button, Prop, PropGroup, input_cmd, props};

use super::GROUP;
use crate::env::config::Time;
use crate::env::{Need, Target};
use crate::routes::Rq;
use crate::routes::work::{pick, quote, reply, strings};

const DATE_FORMATS: &[&str] = &["MDY", "DMY", "YMD", "JUL"];
const DATE_SEPS: &[&str] = &["/", "-", "."];
const TIME_SEPS: &[&str] = &[":", ".", ","];

/// `name (text)`, or the name alone.
fn label(name: &str, text: &str) -> String {
    if text.is_empty() {
        name.to_string()
    } else {
        format!("{name} ({text})")
    }
}

/// The labels of `(value, text)` pairs and the label of `current`.
fn choices(all: &[(String, String)], current: &str) -> (Vec<String>, String) {
    let labels = all.iter().map(|(n, t)| label(n, t)).collect();
    let chosen = all
        .iter()
        .find(|(n, _)| n == current)
        .map_or_else(|| current.to_string(), |(n, t)| label(n, t));
    (labels, chosen)
}

/// The first word of a posted label.
fn word(v: &str) -> String {
    v.split_whitespace().next().unwrap_or("").to_string()
}

/// `+0.004 s` of microseconds.
fn offset(us: i64) -> String {
    let sign = if us < 0 { '-' } else { '+' };
    let a = us.unsigned_abs();
    format!("{sign}{}.{:03} s", a / 1_000_000, a % 1_000_000 / 1000)
}

/// The commands a post asks for: `CHGSYSVAL` for each changed value,
/// `CHGNTPA` when the servers changed (and whether it is there).
fn commands(t: &Time, f: &qsweb::http::Form) -> (Vec<String>, bool) {
    let mut cmds = Vec::new();
    let mut set = |name: &str, new: String, old: &str| {
        if !new.is_empty() && new != old {
            cmds.push(format!("CHGSYSVAL SYSVAL({name}) VALUE({})", quote(&new)));
        }
    };
    set("QTIMZON", word(f.str("tz")), &t.zone);
    set("QDATFMT", f.str("df").trim().to_string(), &t.date_format);
    set("QDATSEP", f.str("dsep").to_string(), &t.date_sep);
    set("QTIMSEP", f.str("tsep").to_string(), &t.time_sep);
    set("QTIMADJ", word(f.str("ta")), &t.adjust);
    let ntp: Vec<String> = f
        .str("ntp")
        .split([',', ' '])
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .collect();
    let ntp_changed = f.has("ntp") && ntp != t.ntp_servers;
    if ntp_changed {
        let list = if ntp.is_empty() {
            "*NONE".to_string()
        } else {
            ntp.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" ")
        };
        cmds.push(format!("CHGNTPA RMTSYS({list})"));
    }
    (cmds, ntp_changed)
}

/// `/time`.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>) -> Response {
    let t = rq.env().config.time(rq.user());
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/home");
        }
        let back = if f.has("ok") { "/home" } else { "/time" };
        let (cmds, ntp) = commands(&t, &f);
        if ntp {
            let target = Target::obj("QSYS", "CHGNTPA", "*CMD");
            if let Err(r) = rq.require(
                Need::IoSysCfg,
                "Change NTP attributes",
                &target,
                &[GROUP, "Time management"],
            ) {
                return r;
            }
        }
        let mut last = None;
        for c in &cmds {
            match rq.env().config.run(rq.user(), c) {
                Err(m) => return reply(rq, Err(m), "/time"),
                ok => last = Some(ok),
            }
        }
        return match last {
            Some(r) => reply(rq, r, back),
            None => rq.redirect(back),
        };
    }
    let (now_d, now_t) = rq.stamp(rq.env().system.now());
    let (zones, zone) = choices(&t.zones, &t.zone);
    let (adjs, adj) = choices(&t.adjusters, &t.adjust);
    let utc = if t.dst {
        format!("{} (daylight saving in effect)", esc(&t.offset))
    } else {
        esc(&t.offset)
    };
    let mut sync = t.last_sync.map_or_else(
        || "Never".to_string(),
        |(at, us)| format!("{}, offset {}", rq.when(at), offset(us)),
    );
    if let Some(why) = &t.last_fail {
        sync = format!("{sync} &nbsp; (last poll: {})", esc(why));
    }
    let ntp_status = format!(
        "{} &nbsp; <a href=\"{}\">Servers</a>",
        if t.ntp_active {
            qsweb::partials::status("Active", Some(true))
        } else {
            qsweb::partials::status("Not active", Some(false))
        },
        esc(&rq.ctx.url("/tcp/servers"))
    );
    let groups = [
        PropGroup {
            title: Some("System time".into()),
            rows: vec![
                Prop::new("Current date/time", format!("{now_d} {now_t}")),
                Prop::new("Time zone (QTIMZON)", pick("tz", &zones, &zone)),
                Prop::new("UTC offset", utc),
                Prop::new(
                    "Date format (QDATFMT)",
                    pick("df", &strings(DATE_FORMATS), &t.date_format),
                ),
                Prop::new(
                    "Date separator",
                    pick("dsep", &strings(DATE_SEPS), &t.date_sep),
                ),
                Prop::new(
                    "Time separator",
                    pick("tsep", &strings(TIME_SEPS), &t.time_sep),
                ),
            ],
        },
        PropGroup {
            title: Some("Time adjustment".into()),
            rows: vec![
                Prop::new("Time adjustment (QTIMADJ)", pick("ta", &adjs, &adj)),
                Prop::new(
                    "NTP servers",
                    input_cmd("ntp", &t.ntp_servers.join(", "), 40),
                ),
                Prop::new("Last synchronization", sync),
                Prop::new("NTP server status", ntp_status),
            ],
        },
    ];
    let body = props(
        &rq.ctx,
        &groups,
        &[
            Button::new("apply", "Apply"),
            Button::new("ok", "OK"),
            Button::new("cancel", "Cancel"),
        ],
    );
    super::page(
        rq,
        &["Time management"],
        "Time management",
        None,
        body,
        "time",
        Some("/time"),
    )
}
