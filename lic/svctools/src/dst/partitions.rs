// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Work with system partitions` (DST option 11, SST option 5). IBM
//! answers on a machine that cannot be partitioned with a panel saying
//! the logical partitioning environment is not supported; QS/400 runs
//! as one system on its hardware (a hypervisor divides the hardware
//! instead), so this is that panel. NEEDS-CODY: the primary's own panel
//! needs a service tools user ID to reach (none is in the oracle's .env).

use console::{Attr, Screen};

use crate::env::DstEnv;
use crate::panel::{Flow, Panel};
use crate::ui;

/// The panel.
pub struct NotSupported;

/// Its title.
pub const TITLE: &str = "Logical Partitioning Environment Not Supported";

const TEXT: [&str; 3] = [
    "This system cannot be divided into logical partitions. It runs as",
    "one system on the hardware it was installed on; a hypervisor that",
    "runs it in a virtual machine divides the hardware instead.",
];

impl Panel for NotSupported {
    fn name(&self) -> &'static str {
        "dst-partitions"
    }

    fn render(&self, env: &mut dyn DstEnv) -> Screen {
        let mut s = Screen::new();
        ui::header(&mut s, TITLE, &env.system_name());
        for (i, line) in TEXT.iter().enumerate() {
            s.put(ui::BODY_ROW + i, ui::BODY_COL, line, Attr::GREEN);
        }
        s.put(
            ui::BODY_ROW + TEXT.len() + 1,
            ui::BODY_COL,
            "Press Enter to return.",
            Attr::GREEN,
        );
        ui::keys(&mut s, "F3=Exit   F12=Cancel");
        s
    }

    fn enter(&mut self, _fields: &[String], _env: &mut dyn DstEnv) -> Flow {
        Flow::Pop
    }
}
