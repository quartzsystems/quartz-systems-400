// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Use Dedicated Service Tools (DST)` menu.

use console::Screen;

use crate::dst::{INVALID_SELECTION, NOT_AUTHORIZED, NOT_AVAILABLE, selection};
use crate::env::DstEnv;
use crate::panel::{Flow, Outcome, Panel};
use crate::ui;
use crate::users::Privilege;

/// The options, numbered as the functional interface has them.
pub const OPTIONS: [(u8, &str); 14] = [
    (1, "Perform an IPL"),
    (2, "Install the operating system"),
    (3, "Work with Licensed Internal Code"),
    (4, "Work with disk units"),
    (5, "Work with DST environment"),
    (6, "Select DST console mode"),
    (7, "Start a service tool"),
    (8, "Perform automatic installation of the operating system"),
    (9, "Work with save storage and restore storage"),
    (10, "Work with remote service support"),
    (11, "Work with system partitions"),
    (12, "Work with system capacity"),
    (13, "Work with system security"),
    (14, "End batch restricted state"),
];

/// The main menu for a signed-on user.
pub struct MainMenu {
    /// The user.
    pub user: String,
    /// What the user may do.
    pub privileges: Privilege,
}

impl MainMenu {
    /// For a signed-on user.
    #[must_use]
    pub fn new(user: String, privileges: Privilege) -> Self {
        Self { user, privileges }
    }

    fn needs(&self, p: Privilege) -> Option<Flow> {
        (!self.privileges.has(p)).then(|| Flow::Error(NOT_AUTHORIZED.into()))
    }
}

impl Panel for MainMenu {
    fn name(&self) -> &'static str {
        "dst-main"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        ui::menu(
            "Use Dedicated Service Tools (DST)",
            &env.system_name(),
            &OPTIONS,
            "F3=Exit   F12=Cancel",
        )
    }

    fn enter(&mut self, fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        let Some(n) = selection(fields) else {
            return Flow::Error(INVALID_SELECTION.into());
        };
        match n {
            1 => self
                .needs(Privilege::IPL)
                .unwrap_or(Flow::Exit(Outcome::PerformIpl)),
            2 => self
                .needs(Privilege::INSTALL)
                .unwrap_or(Flow::Exit(Outcome::InstallOs)),
            8 => self
                .needs(Privilege::INSTALL)
                .unwrap_or(Flow::Exit(Outcome::AutoInstall)),
            4 => self
                .needs(Privilege::DISK_UNITS_OPERATIONS)
                .unwrap_or_else(|| {
                    Flow::Push(Box::new(crate::dst::disk::work_with_disk_units(
                        self.privileges,
                    )))
                }),
            5 => Flow::Push(Box::new(
                crate::dst::environment::work_with_dst_environment(self.privileges),
            )),
            7 => Flow::Push(Box::new(crate::dst::tools::start_a_service_tool(
                self.privileges,
            ))),
            13 => self
                .needs(Privilege::SECURITY)
                .unwrap_or_else(|| Flow::Push(Box::new(crate::dst::security::Security))),
            3 | 6 | 9..=12 | 14 => Flow::Message(NOT_AVAILABLE.into()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::MemEnv;
    use crate::panel::Runner;

    fn f(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn options_route_and_check_privileges() {
        let mut env = MemEnv::new("QS400");
        let mut r = Runner::new(Box::new(MainMenu::new("22222222".into(), Privilege::IPL)));
        let d = r.screen(&mut env).dump();
        assert!(d.contains(" 1. Perform an IPL"));
        assert!(d.contains("14. End batch restricted state"));
        assert_eq!(r.enter(&f(&["2"]), &mut env), None);
        assert!(r.screen(&mut env).dump().contains(NOT_AUTHORIZED));
        assert_eq!(r.enter(&f(&["9"]), &mut env), None);
        assert!(r.screen(&mut env).dump().contains(NOT_AVAILABLE));
        assert_eq!(r.enter(&f(&["x"]), &mut env), None);
        assert!(r.screen(&mut env).dump().contains(INVALID_SELECTION));
        assert_eq!(r.enter(&f(&["1"]), &mut env), Some(Outcome::PerformIpl));
        let mut r = Runner::new(Box::new(MainMenu::new("QSECOFR".into(), Privilege::ALL)));
        assert_eq!(r.enter(&f(&["8"]), &mut env), Some(Outcome::AutoInstall));
        assert_eq!(r.function(12, &mut env), Some(Outcome::Exit));
    }
}
