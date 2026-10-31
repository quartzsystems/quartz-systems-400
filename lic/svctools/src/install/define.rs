// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Define or Change the System at IPL`, reached from `IPL Options`
//! when its "Define or change the system at IPL" choice is Y: the
//! configuration objects, the system values (date and time ones on
//! their own), the network attributes, `Edit Rebuild of Access Paths`
//! (`EDTRBDAP`), `Edit Check Pending Constraints` (`EDTCPCST`), the user
//! profiles and the startup program. The store is open but no job runs
//! yet, so changes go straight to the system values; the rest is shown
//! for checking. F3 goes on with the IPL.

use console::Screen;

use crate::dst::common::{Notice, Options, Submenu};
use crate::dst::tools::Form;
use crate::env::DstEnv;
use crate::panel::{Flow, Outcome, Panel};
use crate::ui::FormLine;

/// The system values `Date and time system values` shows.
pub const DATE_TIME: [&str; 12] = [
    "QCENTURY", "QDATE", "QDAY", "QDAYOFWEEK", "QHOUR", "QMINUTE", "QMONTH", "QSECOND", "QTIME", "QTIMZON",
    "QUTCOFFSET", "QYEAR",
];

/// The options, numbered as the functional interface has them.
pub const OPTIONS: [(u8, &str); 8] = [
    (1, "Configuration commands"),
    (2, "Date and time system values"),
    (3, "System value commands"),
    (4, "Network attributes commands"),
    (5, "Edit rebuild of access paths"),
    (6, "Edit check pending constraints"),
    (7, "Work with user profiles"),
    (8, "Work with the startup program"),
];

/// The menu; F3 and F12 go on with the IPL.
pub struct DefineSystem {
    menu: Submenu,
}

impl Default for DefineSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl DefineSystem {
    /// The menu.
    #[must_use]
    pub fn new() -> Self {
        let actions: [crate::dst::common::Action; 8] = [
            Box::new(|env| Flow::Push(Box::new(configuration(env)))),
            Box::new(|env| Flow::Push(Box::new(sysvals(env, true)))),
            Box::new(|env| Flow::Push(Box::new(sysvals(env, false)))),
            Box::new(|env| Flow::Push(Box::new(network(env)))),
            Box::new(|env| Flow::Push(Box::new(rebuild(env)))),
            Box::new(|env| Flow::Push(Box::new(constraints(env)))),
            Box::new(|env| Flow::Push(Box::new(profiles(env)))),
            Box::new(|env| Flow::Push(Box::new(startup(env)))),
        ];
        let options = OPTIONS
            .iter()
            .zip(actions)
            .map(|(&(n, t), a)| (n, t, Some(a)))
            .collect();
        Self {
            menu: Submenu::new("ipl-define", "Define or Change the System at IPL", options),
        }
    }
}

impl Panel for DefineSystem {
    fn name(&self) -> &'static str {
        "ipl-define"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        self.menu.render(env)
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        self.menu.enter(fields, env)
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 | 12 => Flow::Exit(Outcome::Continue),
            _ => Flow::Stay,
        }
    }
}

fn configuration(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec!["Object      Type      Description".to_string()];
    let objs = env.config_objects();
    if objs.is_empty() {
        lines.push("  (No configuration objects)".into());
    }
    for (n, t, d) in objs {
        lines.push(format!("{n:<10}  {t:<8}  {d}"));
    }
    lines.push(String::new());
    lines.push("Devices, lines and controllers change with their commands once the".into());
    lines.push("system is running (WRKCFGSTS, CHGDEVD, CHGLIND).".into());
    Notice::new("ipl-define-config", "Configuration Objects", lines)
}

/// The system values with 2=Change; `date_time` keeps only those.
fn sysvals(env: &mut dyn DstEnv, date_time: bool) -> Options {
    let vals: Vec<(String, String, String)> = env
        .all_sysvals()
        .into_iter()
        .filter(|(n, _, _)| !date_time || DATE_TIME.contains(&n.as_str()))
        .collect();
    let rows = vals
        .iter()
        .map(|(n, v, t)| {
            let mut v = v.clone();
            v.truncate(20);
            let mut t = t.clone();
            t.truncate(34);
            format!("{n:<10}  {v:<20}  {t}")
        })
        .collect();
    let names: Vec<String> = vals.into_iter().map(|(n, _, _)| n).collect();
    Options::new(
        if date_time { "ipl-define-datetime" } else { "ipl-define-sysvals" },
        if date_time { "Date and Time System Values" } else { "Work with System Values" },
        "Type options, press Enter.  2=Change",
        "Opt  System Value  Current Value         Description",
        rows,
        &[2],
        "F3=Exit   F12=Cancel",
        Box::new(move |chosen, _env| {
            let Some(&(row, _)) = chosen.first() else {
                return Flow::Stay;
            };
            let name = names[row].clone();
            Flow::Push(Box::new(change_sysval(name)))
        }),
        None,
    )
}

fn change_sysval(name: String) -> Form {
    Form::new(
        "ipl-define-chgsysval",
        "Change System Value",
        vec![FormLine::new("New value", 40, "")],
        vec![format!("System value {name}: the value is checked as CHGSYSVAL checks it.")],
        Box::new(move |fields, env| {
            let v = fields.first().map_or("", |s| s.trim());
            match env.set_sysval(&name, v) {
                Ok(m) => Flow::Unwind(2, Some((m, false))),
                Err(e) => Flow::Error(e),
            }
        }),
    )
}

fn network(env: &mut dyn DstEnv) -> Notice {
    let mut lines: Vec<String> = env
        .network_attributes()
        .into_iter()
        .map(|(k, v)| format!("{k:<12}  {v}"))
        .collect();
    if lines.is_empty() {
        lines.push("  (No network attributes)".into());
    }
    lines.push(String::new());
    lines.push("CHGNETA changes them once the system is running.".into());
    Notice::new("ipl-define-neta", "Network Attributes", lines)
}

fn rebuild(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec![
        "Access paths the IPL rebuilds, in sequence. Recovery after an abnormal end".to_string(),
        "rebuilds an access path whose journal or checksum cannot vouch for it.".into(),
        String::new(),
        "Seq  Status      File        Library     Member      Run time".into(),
    ];
    let rows = env.rebuild_paths();
    if rows.is_empty() {
        lines.push("  (No access paths need rebuilding)".into());
    }
    lines.extend(rows);
    Notice::new("ipl-define-edtrbdap", "Edit Rebuild of Access Paths", lines)
}

fn constraints(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec![
        "Constraints the IPL verifies, in sequence.".to_string(),
        String::new(),
        "Seq  Status      Constraint  File        Library     Verify time".into(),
    ];
    let rows = env.pending_constraints();
    if rows.is_empty() {
        lines.push("  (No constraints are check pending)".into());
    }
    lines.extend(rows);
    Notice::new("ipl-define-edtcpcst", "Edit Check Pending Constraints", lines)
}

fn profiles(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec!["User profile  Description".to_string()];
    let users = env.user_profiles();
    if users.is_empty() {
        lines.push("  (No user profiles)".into());
    }
    for (n, t) in users {
        lines.push(format!("{n:<12}  {t}"));
    }
    lines.push(String::new());
    lines.push("CHGUSRPRF changes a profile once the system is running; a lost".into());
    lines.push("QSECOFR password is reset from DST.".into());
    Notice::new("ipl-define-usrprf", "User Profiles", lines)
}

fn startup(env: &mut dyn DstEnv) -> Form {
    let current = env
        .all_sysvals()
        .into_iter()
        .find(|(n, _, _)| n == "QSTRUPPGM")
        .map(|(_, v, _)| v)
        .unwrap_or_default();
    Form::new(
        "ipl-define-strup",
        "Work with the Startup Program",
        vec![FormLine::new("Startup program (QSTRUPPGM)", 20, current)],
        vec![
            "Program and library, or *NONE. An escape from the program does not",
            "stop the IPL: the system goes on to sign-on and QSYSOPR is told.",
        ],
        Box::new(|fields, env| {
            let v = fields.first().map_or("", |s| s.trim()).to_ascii_uppercase();
            match env.set_sysval("QSTRUPPGM", &v) {
                Ok(m) => Flow::Unwind(1, Some((m, false))),
                Err(e) => Flow::Error(e),
            }
        }),
    )
}

/// Shown in an attended IPL after an abnormal end: go on, or use DST
/// first (`Outcome::Dst`).
pub struct AbnormalEnd;

impl Panel for AbnormalEnd {
    fn name(&self) -> &'static str {
        "ipl-abnormal"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = crate::ui::menu(
            "Previous System End Was Abnormal",
            &env.system_name(),
            &[(1, "Continue the IPL"), (2, "Use Dedicated Service Tools (DST)")],
            "F3=Exit   F12=Cancel",
        );
        s.put(
            crate::ui::BODY_ROW + 4,
            crate::ui::BODY_COL,
            "The system did not end normally. The IPL recovers the object store and",
            console::Attr::GREEN,
        );
        s.put(
            crate::ui::BODY_ROW + 5,
            crate::ui::BODY_COL,
            "the journals; DST can look at the disk units first.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        match crate::dst::selection(fields) {
            Some(1) => Flow::Exit(Outcome::Continue),
            Some(2) => Flow::Exit(Outcome::Dst),
            _ => Flow::Error(crate::dst::INVALID_SELECTION.into()),
        }
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 | 12 => Flow::Exit(Outcome::Continue),
            _ => Flow::Stay,
        }
    }
}

/// The reclaim of storage an IPL runs when the store says it must:
/// the screen while it runs, then its report.
#[must_use]
pub fn reclaim_screen(system: &str, report: Option<&str>) -> Screen {
    let mut lines = vec![
        "The object store was marked as needing a reclaim of storage. The IPL".to_string(),
        "runs RCLSTG now: every object is checked against its library, lost".into(),
        "objects go to QRCL, damaged ones are marked.".into(),
        String::new(),
    ];
    match report {
        None => lines.push("Reclaim storage in progress . . . . :  Running".into()),
        Some(r) => {
            lines.push("Reclaim storage in progress . . . . :  Complete".into());
            lines.push(String::new());
            lines.extend(r.lines().take(10).map(str::to_string));
        }
    }
    crate::ui::text("Reclaim Storage", system, &lines, None, "")
}
