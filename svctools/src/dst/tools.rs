// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Start a service tool`: the product activity log, display/alter/dump
//! (display only), the LIC log, and the hardware service manager.

use console::Screen;

use crate::dst::NOT_AUTHORIZED;
use crate::dst::common::{Notice, Submenu};
use crate::env::{DstEnv, ResourceRow};
use crate::panel::{Flow, Panel};
use crate::ui::{self, FormLine};
use crate::users::Privilege;

/// The `Start a Service Tool` menu.
#[must_use]
pub fn start_a_service_tool(privileges: Privilege) -> Submenu {
    let logs = privileges.has(Privilege::LOGS);
    let dump = privileges.has(Privilege::DISPLAY_ALTER_DUMP);
    let hw = privileges.has(Privilege::HARDWARE);
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
            (2, "Trace Licensed Internal Code", None),
            (3, "Work with communications trace", None),
            (
                4,
                "Display/Alter/Dump",
                Some(Box::new(move |_env| {
                    guard(dump, Flow::Push(Box::new(DisplayAlterDump)))
                })),
            ),
            (
                5,
                "Licensed Internal Code log",
                Some(Box::new(move |env| {
                    guard(logs, Flow::Push(Box::new(lic_log(env))))
                })),
            ),
            (6, "Main storage dump manager", None),
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

fn lic_log(env: &mut dyn DstEnv) -> Notice {
    let mut lines = env.lic_log();
    if lines.is_empty() {
        lines.push("  (No entries)".into());
    }
    Notice::new("dst-lic-log", "Licensed Internal Code Log", lines)
}

/// Display/Alter/Dump: a page of a unit, or an object header, in hex.
/// Alter is refused in this release.
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
            "Display only: altering storage is not available in this release.",
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

/// The `Hardware Service Manager` menu: the three views of the table.
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
                Some(Box::new(|env| {
                    Flow::Push(Box::new(resources(
                        env,
                        "dst-hsm-logical",
                        "Logical Hardware Resources",
                        |_| true,
                        false,
                    )))
                })),
            ),
            (
                3,
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
        ],
    )
}

fn resources(
    env: &mut dyn DstEnv,
    name: &'static str,
    title: &'static str,
    keep: fn(&ResourceRow) -> bool,
    with_location: bool,
) -> Notice {
    let mut lines = vec![if with_location {
        "Resource  Type      Status        Location          Description".to_string()
    } else {
        "Resource  Type      Status        Description".to_string()
    }];
    let rows: Vec<ResourceRow> = env.resources().into_iter().filter(keep).collect();
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
