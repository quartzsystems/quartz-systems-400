// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `System Service Tools (SST)` menu and the sign-on in front of
//! it. Option numbers and titles are the functional interface; each
//! option opens the DST panels for it.

use console::Screen;
use svctools::dst::common::{Notice, Submenu};
use svctools::dst::signon::{SignOn, Tool};
use svctools::dst::{INVALID_SELECTION, NOT_AUTHORIZED, NOT_AVAILABLE, selection};
use svctools::panel::{Flow, Outcome, Panel, Runner};
use svctools::ui;
use svctools::{DstEnv, Privilege};

/// SST's sign-on title, password note and menu.
pub const TOOL: Tool = Tool {
    title: "Start Service Tools (STRSST) Sign On",
    expired: "Password has expired and must be changed before SST can be used",
    home,
};

fn home(user: String, privileges: Privilege) -> Box<dyn Panel> {
    Box::new(SstMenu::new(user, privileges))
}

/// SST, starting at its sign-on.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(SignOn::new(TOOL)))
}

/// The options, numbered as the functional interface has them.
pub const OPTIONS: [(u8, &str); 8] = [
    (1, "Start a service tool"),
    (2, "Work with active service tools"),
    (3, "Work with disk units"),
    (4, "Work with diskette data recovery"),
    (5, "Work with system partitions"),
    (6, "Work with system capacity"),
    (7, "Work with system security"),
    (8, "Work with service tools user IDs and Devices"),
];

/// The main menu for a signed-on user.
pub struct SstMenu {
    /// The user.
    pub user: String,
    /// What the user may do.
    pub privileges: Privilege,
}

impl SstMenu {
    /// For a signed-on user.
    #[must_use]
    pub fn new(user: String, privileges: Privilege) -> Self {
        Self { user, privileges }
    }

    fn needs(&self, p: Privilege, then: impl FnOnce() -> Flow) -> Flow {
        if self.privileges.has(p) {
            then()
        } else {
            Flow::Error(NOT_AUTHORIZED.into())
        }
    }
}

impl Panel for SstMenu {
    fn name(&self) -> &'static str {
        "sst-main"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::menu(
            "System Service Tools (SST)",
            &env.system_name(),
            &OPTIONS,
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], env: &mut dyn DstEnv) -> Flow {
        let Some(n) = selection(fields) else {
            return Flow::Error(INVALID_SELECTION.into());
        };
        let p = self.privileges;
        match n {
            1 => Flow::Push(Box::new(svctools::dst::tools::start_a_service_tool(p))),
            2 => Flow::Push(Box::new(active_tools(env))),
            3 => self.needs(Privilege::DISK_UNITS_OPERATIONS, || {
                Flow::Push(Box::new(svctools::dst::disk::work_with_disk_units(p)))
            }),
            6 => Flow::Push(Box::new(crate::capacity::work_with_system_capacity())),
            7 => self.needs(Privilege::SECURITY, || {
                Flow::Push(Box::new(svctools::dst::security::Security))
            }),
            8 => Flow::Push(Box::new(user_ids_and_devices(p))),
            4 | 5 => Flow::Message(NOT_AVAILABLE.into()),
            _ => Flow::Error(INVALID_SELECTION.into()),
        }
    }

    fn function(&mut self, key: u8, _env: &mut dyn DstEnv) -> Flow {
        match key {
            3 | 12 => Flow::Exit(Outcome::Exit),
            _ => Flow::Stay,
        }
    }
}

/// `Work with Active Service Tools`: this session.
fn active_tools(env: &mut dyn DstEnv) -> Notice {
    Notice::new(
        "sst-active",
        "Work with Active Service Tools",
        vec![
            "Service tool                        Started by".into(),
            format!("System service tools (SST)          {}", env.console_type()),
        ],
    )
}

/// `Work with Service Tools User IDs and Devices`.
fn user_ids_and_devices(privileges: Privilege) -> Submenu {
    let can = privileges.has(Privilege::USER_IDS);
    Submenu::new(
        "sst-users-devices",
        "Work with Service Tools User IDs and Devices",
        vec![
            (
                1,
                "Service tools user IDs",
                Some(Box::new(move |env: &mut dyn DstEnv| {
                    if can {
                        Flow::Push(Box::new(svctools::dst::environment::user_list(env)))
                    } else {
                        Flow::Error(NOT_AUTHORIZED.into())
                    }
                })),
            ),
            (
                2,
                "Service tools device IDs",
                Some(Box::new(move |env: &mut dyn DstEnv| {
                    if !can {
                        return Flow::Error(NOT_AUTHORIZED.into());
                    }
                    let mut lines = vec!["Device ID   Status     Description".to_string()];
                    for (n, s, d) in env.device_ids() {
                        lines.push(format!("{n:<10}  {s:<9}  {d}"));
                    }
                    lines.push(String::new());
                    lines.push("Service tools are reached from the system console and from".into());
                    lines.push("STRSST in a job; there are no LAN service tools devices.".into());
                    Flow::Push(Box::new(Notice::new(
                        "sst-device-ids",
                        "Work with Service Tools Device IDs",
                        lines,
                    )))
                })),
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use svctools::env::MemEnv;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn sign_on_reaches_the_menu_and_options_route() {
        let mut env = MemEnv::new("QS400");
        let mut r = start();
        assert!(r.screen(&mut env).dump().contains("(STRSST) Sign On"));
        r.enter(&f(&["qsrv", "QSRV"]), &mut env);
        assert_eq!(r.top(), "sst-main");
        r.enter(&f(&["1"]), &mut env);
        assert_eq!(r.top(), "dst-tools");
        r.function(12, &mut env);
        r.enter(&f(&["8"]), &mut env);
        r.enter(&f(&["1"]), &mut env);
        // QSRV may not work with user IDs.
        assert!(r.screen(&mut env).dump().contains(NOT_AUTHORIZED));
        r.function(3, &mut env);
        r.enter(&f(&["4"]), &mut env);
        assert!(r.screen(&mut env).dump().contains(NOT_AVAILABLE));
        assert_eq!(r.function(3, &mut env), Some(Outcome::Exit));
    }

    #[test]
    fn an_expired_password_comes_back_to_sst() {
        let mut env = MemEnv::new("QS400");
        let mut r = start();
        r.enter(&f(&["QSECOFR", "QSECOFR"]), &mut env);
        assert_eq!(r.top(), "dst-change-password");
        assert!(r.screen(&mut env).dump().contains("before SST can be used"));
        r.enter(&f(&["QSECOFR", "QSECOFR", "Quartz1", "Quartz1"]), &mut env);
        assert_eq!(r.top(), "sst-main");
    }
}
