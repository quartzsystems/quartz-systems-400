// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Start a service tool`: the product activity log, the Licensed
//! Internal Code trace, display/alter/dump, the LIC log, the main
//! storage dump manager and the hardware service manager
//! (`docs/SERVICE.md`). Titles and option numbers are IBM's.

use console::Screen;

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Confirm, Notice, Options, Submenu};
use crate::env::{DstEnv, ResourceRow, TraceSpec};
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};
use crate::users::Privilege;

/// What a form does with its fields.
pub type Submit = Box<dyn FnMut(&[String], &mut dyn DstEnv) -> Flow>;

/// A form of labelled fields with a note under them.
pub struct Form {
    name: &'static str,
    title: &'static str,
    lines: Vec<FormLine>,
    note: Vec<&'static str>,
    submit: Submit,
}

impl Form {
    /// With these fields, note lines and action.
    #[must_use]
    pub fn new(
        name: &'static str,
        title: &'static str,
        lines: Vec<FormLine>,
        note: Vec<&'static str>,
        submit: Submit,
    ) -> Self {
        Self {
            name,
            title,
            lines,
            note,
            submit,
        }
    }
}

impl Panel for Form {
    fn name(&self) -> &'static str {
        self.name
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::form(self.title, &env.system_name(), &self.lines, "F3=Exit   F12=Cancel");
        for (i, n) in self.note.iter().enumerate() {
            s.put(
                ui::BODY_ROW + self.lines.len() + 1 + i,
                ui::BODY_COL,
                n,
                console::Attr::GREEN,
            );
        }
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        (self.submit)(fields, env)
    }
}

fn field(fields: &[String], i: usize) -> String {
    fields.get(i).map_or("", |s| s.trim()).to_string()
}

/// The `Start a Service Tool` menu.
#[must_use]
pub fn start_a_service_tool(privileges: Privilege) -> Submenu {
    let logs = privileges.has(Privilege::LOGS);
    let dump = privileges.has(Privilege::DISPLAY_ALTER_DUMP);
    let hw = privileges.has(Privilege::HARDWARE);
    let dt = privileges.has(Privilege::DUMPS_AND_TRACES);
    let guard = |allowed: bool, then: Flow| {
        if allowed {
            then
        } else {
            Flow::Error(NOT_AUTHORIZED.into())
        }
    };
    Submenu::new(
        "dst-tools",
        "Start a Service Tool",
        vec![
            (
                1,
                "Product activity log",
                Some(Box::new(move |env| {
                    guard(logs, Flow::Push(Box::new(activity_log(env))))
                })),
            ),
            (
                2,
                "Trace Licensed Internal Code",
                Some(Box::new(move |env| guard(dt, Flow::Push(Box::new(work_with_traces(env)))))),
            ),
            (3, "Work with communications trace", None),
            (
                4,
                "Display/Alter/Dump",
                Some(Box::new(move |_env| {
                    guard(dump, Flow::Push(Box::new(display_alter_dump())))
                })),
            ),
            (
                5,
                "Licensed Internal Code log",
                Some(Box::new(move |_env| guard(logs, Flow::Push(Box::new(lic_log_menu()))))),
            ),
            (
                6,
                "Main storage dump manager",
                Some(Box::new(move |_env| guard(dt, Flow::Push(Box::new(dump_manager()))))),
            ),
            (
                7,
                "Hardware service manager",
                Some(Box::new(move |_env| {
                    guard(hw, Flow::Push(Box::new(hardware_service_manager())))
                })),
            ),
        ],
    )
}

fn activity_log(env: &mut dyn DstEnv) -> Notice {
    let mut lines = vec!["Time         Code      Description".to_string()];
    let entries = env.activity_log();
    if entries.is_empty() {
        lines.push("  (No entries)".into());
    }
    for e in entries.iter().rev() {
        lines.push(format!("{:<12} {:<9} {}", e.time, e.code, e.text));
    }
    Notice::new("dst-pal", "Product Activity Log", lines)
}

// --- the Licensed Internal Code log

/// `Licensed Internal Code Log`: the event log by major code, and the
/// IPL history.
#[must_use]
pub fn lic_log_menu() -> Submenu {
    Submenu::new(
        "dst-lic-log-menu",
        "Licensed Internal Code Log",
        vec![
            (
                1,
                "Select entries from the Licensed Internal Code log",
                Some(Box::new(|_env| Flow::Push(Box::new(lic_log_selection())))),
            ),
            (
                2,
                "Display the IPL history",
                Some(Box::new(|env| Flow::Push(Box::new(lic_log(env))))),
            ),
        ],
    )
}

fn lic_log_selection() -> Form {
    Form::new(
        "dst-lic-log-select",
        "Specify Licensed Internal Code Log Selection Criteria",
        vec![FormLine::new("Major code (blank for all)", 4, "")],
        vec![
            "Major codes: 0100 IPL, 0200 storage service, 0300 jobs, 0400 problems,",
            "0500 dumps, 0600 traces, 0700 service tools, 0800 disk units.",
        ],
        Box::new(|fields, env| {
            let m = field(fields, 0);
            let major = if m.is_empty() {
                0
            } else {
                match u16::from_str_radix(&m, 16) {
                    Ok(v) => v,
                    Err(_) => return Flow::Error(format!("Major code {m} is not four hex digits")),
                }
            };
            let mut lines = vec!["   Seq Date       Time     Major Minor Job    Description".to_string()];
            let events = env.lic_events(major);
            if events.is_empty() {
                lines.push("  (No entries)".into());
            }
            lines.extend(events);
            Flow::Push(Box::new(Notice::new("dst-lic-log-entries", "Licensed Internal Code Log", lines)))
        }),
    )
}

fn lic_log(env: &mut dyn DstEnv) -> Notice {
    let mut lines = env.lic_log();
    if lines.is_empty() {
        lines.push("  (No entries)".into());
    }
    Notice::new("dst-lic-log", "Licensed Internal Code Log", lines)
}

// --- the Licensed Internal Code trace

/// `Work with Traces`: the traces with 2=Stop, 4=Delete, 5=Display;
/// F6 starts one.
pub fn work_with_traces(env: &mut dyn DstEnv) -> Panel2 {
    let rows = match env.traces() {
        Ok(t) => t
            .into_iter()
            .map(|t| format!("{:<10}  {:<4}  {:<8}  {:>8}", t.name, t.kind, t.status, t.records))
            .collect(),
        Err(e) => return Panel2::Notice(Notice::new("dst-trace-none", "Trace Licensed Internal Code", vec![e])),
    };
    Panel2::Options(Options::new(
        "dst-trace",
        "Work with Traces",
        "Type options, press Enter.  2=Stop   4=Delete   5=Display",
        "Opt  Trace       Type  Status     Records",
        rows,
        &[2, 4, 5],
        "F3=Exit   F6=Start trace   F12=Cancel",
        Box::new(|chosen, env| {
            let names: Vec<String> = env.traces().unwrap_or_default().into_iter().map(|t| t.name).collect();
            for &(row, opt) in chosen {
                let Some(name) = names.get(row) else {
                    continue;
                };
                let r = match opt {
                    2 => env.stop_trace(name),
                    4 => env.delete_trace(name),
                    _ => match env.trace_lines(name) {
                        Ok(lines) => {
                            return Flow::Push(Box::new(Notice::new("dst-trace-display", "Display Trace", lines)));
                        }
                        Err(e) => Err(e),
                    },
                };
                if let Err(e) = r {
                    return Flow::Error(e);
                }
            }
            Flow::Replace(Box::new(work_with_traces(env)))
        }),
        Some(Box::new(|_env| Flow::Push(Box::new(start_trace())))),
    ))
}

/// A panel that is a list or, when there is nothing to list, a notice.
pub enum Panel2 {
    /// The list.
    Options(Options),
    /// Why there is none.
    Notice(Notice),
}

impl Panel for Panel2 {
    fn name(&self) -> &'static str {
        match self {
            Self::Options(o) => o.name(),
            Self::Notice(n) => n.name(),
        }
    }
    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        match self {
            Self::Options(o) => o.render(env),
            Self::Notice(n) => n.render(env),
        }
    }
    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        match self {
            Self::Options(o) => o.enter(fields, env),
            Self::Notice(n) => n.enter(fields, env),
        }
    }
    fn function(&mut self, key: u8, env: &mut dyn DstEnv) -> Flow {
        match self {
            Self::Options(o) => o.function(key, env),
            Self::Notice(n) => n.function(key, env),
        }
    }
}

fn start_trace() -> Form {
    Form::new(
        "dst-trace-start",
        "Start a Licensed Internal Code Trace",
        vec![
            FormLine::new("Trace table name", 10, "QSTRCTBL"),
            FormLine::new("Trace points", 40, "*ALL"),
            FormLine::new("Job number (0 for all)", 6, "0"),
            FormLine::new("Trace table size (KiB)", 7, "1024"),
            FormLine::new("Wrap when full (Y, N)", 1, "Y"),
        ],
        vec![
            "Categories: CALL CMD REQ MSG IO JOB PRB DMP LOG, or *ALL.",
            "The trace collects the storage service's trace points until stopped.",
        ],
        Box::new(|fields, env| {
            let Ok(job) = field(fields, 2).parse::<u32>() else {
                return Flow::Error("Type a job number, 0 for every job".into());
            };
            let size_kib = field(fields, 3).parse().unwrap_or(1024);
            let spec = TraceSpec {
                name: field(fields, 0).to_ascii_uppercase(),
                categories: field(fields, 1).to_ascii_uppercase(),
                job,
                size_kib,
                wrap: !field(fields, 4).eq_ignore_ascii_case("N"),
            };
            match env.start_trace(&spec) {
                Ok(()) => Flow::Unwind(2, Some((format!("Trace {} started", spec.name), false))),
                Err(e) => Flow::Error(e),
            }
        }),
    )
}

// --- display/alter/dump

/// `Display/Alter/Dump`.
#[must_use]
pub fn display_alter_dump() -> Submenu {
    Submenu::new(
        "dst-dad-menu",
        "Display/Alter/Dump Output Device",
        vec![
            (1, "Display/Alter storage", Some(Box::new(|_env| Flow::Push(Box::new(DisplayAlterDump))))),
            (2, "Dump to printer", Some(Box::new(|_env| Flow::Push(Box::new(dump_to_printer()))))),
            (3, "Alter object storage", Some(Box::new(|_env| Flow::Push(Box::new(alter_form()))))),
        ],
    )
}

/// Display storage: a page of a unit, or an object header, in hex.
pub struct DisplayAlterDump;

impl Panel for DisplayAlterDump {
    fn name(&self) -> &'static str {
        "dst-dad"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = ui::form(
            "Display/Alter/Dump",
            &env.system_name(),
            &[
                FormLine::new("Disk unit number (from 1)", 4, ""),
                FormLine::new("Page number on the unit", 12, ""),
                FormLine::new("Or object identifier", 40, ""),
            ],
            "F3=Exit   F12=Cancel",
        );
        s.put(
            ui::BODY_ROW + 4,
            ui::BODY_COL,
            "Altering is option 3 of the previous menu, for an object's pages.",
            console::Attr::GREEN,
        );
        s
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let unit = fields.first().map_or("", String::as_str).trim();
        let page = fields.get(1).map_or("", String::as_str).trim();
        let id = fields.get(2).map_or("", String::as_str).trim();
        let (title, bytes) = if id.is_empty() {
            let (Ok(u), Ok(p)) = (unit.parse::<u16>(), page.parse::<u64>()) else {
                return Flow::Error(
                    "Type a unit number and a page number, or an object identifier".into(),
                );
            };
            if u == 0 {
                return Flow::Error("Unit numbers start at 1".into());
            }
            match env.read_page(u - 1, p) {
                Ok(b) => (format!("Unit {u} page {p}"), b),
                Err(e) => return Flow::Error(e),
            }
        } else {
            match env.object_header(id) {
                Ok(b) => (format!("Object header {id}"), b),
                Err(e) => return Flow::Error(e),
            }
        };
        Flow::Push(Box::new(Notice::new(
            "dst-dad-hex",
            "Display Storage",
            hex_lines(&title, &bytes),
        )))
    }
}

fn dump_to_printer() -> Form {
    Form::new(
        "dst-dad-print",
        "Dump to Printer",
        vec![
            FormLine::new("Disk unit number (from 1)", 4, ""),
            FormLine::new("Page number on the unit", 12, ""),
            FormLine::new("Or object identifier", 40, ""),
        ],
        vec!["The storage is printed in hex to spooled file QPCSMPRT."],
        Box::new(|fields, env| match env.print_storage(&field(fields, 0), &field(fields, 1), &field(fields, 2)) {
            Ok(m) => Flow::Message(m),
            Err(e) => Flow::Error(e),
        }),
    )
}

fn alter_form() -> Form {
    Form::new(
        "dst-dad-alter",
        "Alter Object Storage",
        vec![
            FormLine::new("Object identifier", 40, ""),
            FormLine::new("Page number", 12, ""),
            FormLine::new("Offset in the page (hex)", 4, ""),
            FormLine::new("New bytes (hex)", 32, ""),
        ],
        vec![
            "The page must be the object's header page or one of its own.",
            "The change is written at once and recorded in the LIC log.",
        ],
        Box::new(|fields, _env| {
            let (id, page, off, hex) = (field(fields, 0), field(fields, 1), field(fields, 2), field(fields, 3));
            let Ok(p) = page.parse::<u64>() else {
                return Flow::Error("Type the page number".into());
            };
            let lines = vec![
                format!("Object . . . . : {id}"),
                format!("Page . . . . . : {p}"),
                format!("Offset . . . . : {off}"),
                format!("New bytes  . . : {hex}"),
                String::new(),
                "Press F10 to alter the storage, F12 to cancel.".into(),
            ];
            Flow::Push(Box::new(Confirm::new(
                "dst-dad-alter-confirm",
                "Confirm Alter Storage",
                lines,
                Box::new(move |env| env.alter_storage(&id, p, &off, &hex)),
            )))
        }),
    )
}

/// Hex dump lines: offset, 16 bytes, printable ASCII.
#[must_use]
pub fn hex_lines(title: &str, bytes: &[u8]) -> Vec<String> {
    let mut lines = vec![title.to_string()];
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
        let text: String = chunk
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        lines.push(format!("{:06X}  {:<47}  {text}", i * 16, hex.join(" ")));
    }
    lines
}

// --- the main storage dump manager

/// `Main Storage Dump Manager`.
#[must_use]
pub fn dump_manager() -> Submenu {
    Submenu::new(
        "dst-msd",
        "Main Storage Dump Manager",
        vec![
            (
                1,
                "Take a main storage dump",
                Some(Box::new(|_env| {
                    Flow::Push(Box::new(Confirm::new(
                        "dst-msd-take",
                        "Confirm Main Storage Dump",
                        vec![
                            "A copy of the storage service's state, the job table, the IPL".into(),
                            "history and the Licensed Internal Code log is kept as a dump".into(),
                            "in ASP 1, and a problem is logged for it.".into(),
                            String::new(),
                            "Press F10 to take the dump, F12 to cancel.".into(),
                        ],
                        Box::new(|env| {
                            env.take_dump("Taken with the main storage dump manager")
                                .map(|n| format!("Main storage dump {n} taken"))
                        }),
                    )))
                })),
            ),
            (
                2,
                "Work with main storage dumps",
                Some(Box::new(|env| Flow::Push(Box::new(work_with_dumps(env))))),
            ),
            (3, "Copy to media", None),
        ],
    )
}

/// The dumps kept, with 4=Delete, 5=Display, 6=Copy to media.
pub fn work_with_dumps(env: &mut dyn DstEnv) -> Panel2 {
    let rows = match env.dumps() {
        Ok(d) => d
            .into_iter()
            .map(|d| {
                let (y, mo, day, hh, mm, _) = civil(d.taken);
                format!("{:<10}  {y:04}-{mo:02}-{day:02} {hh:02}:{mm:02}  {:>8}  {}", d.name, d.size, d.text)
            })
            .collect(),
        Err(e) => return Panel2::Notice(Notice::new("dst-msd-none", "Work with Main Storage Dumps", vec![e])),
    };
    Panel2::Options(Options::new(
        "dst-msd-list",
        "Work with Main Storage Dumps",
        "Type options, press Enter.  4=Delete   5=Display   6=Copy to media",
        "Opt  Dump        Taken                Bytes  Reason",
        rows,
        &[4, 5, 6],
        "F3=Exit   F12=Cancel",
        Box::new(|chosen, env| {
            let names: Vec<String> = env.dumps().unwrap_or_default().into_iter().map(|d| d.name).collect();
            for &(row, opt) in chosen {
                let Some(name) = names.get(row) else {
                    continue;
                };
                let r = match opt {
                    4 => env.delete_dump(name),
                    6 => env.copy_dump(name),
                    _ => match env.dump_lines(name) {
                        Ok(lines) => {
                            return Flow::Push(Box::new(Notice::new("dst-msd-display", "Display Main Storage Dump", lines)));
                        }
                        Err(e) => Err(e),
                    },
                };
                if let Err(e) = r {
                    return Flow::Error(e);
                }
            }
            Flow::Replace(Box::new(work_with_dumps(env)))
        }),
        None,
    ))
}

/// Civil time of Unix seconds, UTC.
#[allow(clippy::many_single_char_names)] // the civil-from-days names
fn civil(t: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = i64::try_from(t / 86_400).unwrap_or(0);
    let secs = t % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let y = yoe + era * 400 + i64::from(m <= 2);
    let s = u32::try_from(secs).unwrap_or(0);
    (y, m, d, s / 3600, s / 60 % 60, s % 60)
}

// --- the hardware service manager

/// The `Hardware Service Manager` menu; F6 prints the configuration.
#[must_use]
pub fn hardware_service_manager() -> Submenu {
    Submenu::new(
        "dst-hsm",
        "Hardware Service Manager",
        vec![
            (
                1,
                "Packaging hardware resources (systems, frames, cards, ...)",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(resources(
                        env,
                        "dst-hsm-packaging",
                        "Packaging Hardware Resources",
                        |_| true,
                        true,
                    )))
                })),
            ),
            (
                2,
                "Logical hardware resources (buses, IOPs, controllers, ...)",
                Some(Box::new(|_env| Flow::Push(Box::new(logical_menu())))),
            ),
            (
                3,
                "Locate resource by resource name",
                Some(Box::new(|_env| Flow::Push(Box::new(locate())))),
            ),
            (
                4,
                "Failed and non-reporting hardware resources",
                Some(Box::new(|env| {
                    Flow::Push(Box::new(resources(
                        env,
                        "dst-hsm-failed",
                        "Failed and Non-Reporting Hardware Resources",
                        |r| r.status != "Operational",
                        false,
                    )))
                })),
            ),
            (6, "Work with service action log", None),
            (7, "Display label location work sheet", None),
            (8, "Device Concurrent Maintenance", None),
        ],
    )
    .with_key(
        6,
        "F3=Exit   F6=Print configuration   F12=Cancel",
        Box::new(|env| match env.print_hardware() {
            Ok(m) => Flow::Message(m),
            Err(e) => Flow::Error(e),
        }),
    )
}

/// The class a resource kind belongs to: `*CMN`, `*STG`, `*TAP`, or
/// none.
#[must_use]
pub fn class_of(kind: &str) -> &'static str {
    match kind {
        "Lan" | "Comm" | "Port" => "*CMN",
        "Storage" | "Disk" => "*STG",
        "Optical" | "Tape" => "*TAP",
        _ => "",
    }
}

fn logical_menu() -> Submenu {
    let by = |name: &'static str, title: &'static str, class: &'static str| -> crate::dst::common::Action {
        Box::new(move |env| {
            let rows: Vec<ResourceRow> = env.resources().into_iter().filter(|r| class.is_empty() || class_of(&r.kind) == class).collect();
            Flow::Push(Box::new(resource_notice(rows, name, title, false)))
        })
    };
    Submenu::new(
        "dst-hsm-logical-menu",
        "Logical Hardware Resources",
        vec![
            (1, "All logical hardware resources", Some(by("dst-hsm-logical", "Logical Hardware Resources", ""))),
            (2, "Communications resources (*CMN)", Some(by("dst-hsm-cmn", "Communications Resources", "*CMN"))),
            (3, "Storage resources (*STG)", Some(by("dst-hsm-stg", "Storage Resources", "*STG"))),
            (4, "Tape and optical resources (*TAP)", Some(by("dst-hsm-tap", "Tape and Optical Resources", "*TAP"))),
        ],
    )
}

fn locate() -> Form {
    Form::new(
        "dst-hsm-locate",
        "Locate Resource by Resource Name",
        vec![FormLine::new("Resource name", 10, "")],
        vec!["A name ending in * finds every resource that starts so."],
        Box::new(|fields, env| {
            let want = field(fields, 0).to_ascii_uppercase();
            if want.is_empty() {
                return Flow::Error("Type a resource name".into());
            }
            let rows: Vec<ResourceRow> = env
                .resources()
                .into_iter()
                .filter(|r| match want.strip_suffix('*') {
                    Some(p) => r.name.starts_with(p),
                    None => r.name == want,
                })
                .collect();
            if rows.is_empty() {
                return Flow::Error(format!("No resource is called {want}"));
            }
            Flow::Push(Box::new(resource_notice(rows, "dst-hsm-located", "Resources Located", true)))
        }),
    )
}

fn resources(
    env: &mut dyn DstEnv,
    name: &'static str,
    title: &'static str,
    keep: fn(&ResourceRow) -> bool,
    with_location: bool,
) -> Notice {
    let rows: Vec<ResourceRow> = env.resources().into_iter().filter(keep).collect();
    resource_notice(rows, name, title, with_location)
}

fn resource_notice(rows: Vec<ResourceRow>, name: &'static str, title: &'static str, with_location: bool) -> Notice {
    let mut lines = vec![if with_location {
        "Resource  Type      Status        Location          Description".to_string()
    } else {
        "Resource  Type      Status        Description".to_string()
    }];
    if rows.is_empty() {
        lines.push("  (No resources)".into());
    }
    for r in rows {
        lines.push(if with_location {
            format!(
                "{:<9} {:<9} {:<13} {:<17} {}",
                r.name, r.kind, r.status, r.location, r.description
            )
        } else {
            format!(
                "{:<9} {:<9} {:<13} {}",
                r.name, r.kind, r.status, r.description
            )
        });
    }
    Notice::new(name, title, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_and_classes() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil(1_793_318_400 + 3661), (2026, 10, 30, 1, 1, 1));
        assert_eq!(class_of("Lan"), "*CMN");
        assert_eq!(class_of("Disk"), "*STG");
        assert_eq!(class_of("Optical"), "*TAP");
        assert_eq!(class_of("Bridge"), "");
    }
}
