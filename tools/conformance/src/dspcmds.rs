// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 82: display commands from the main menu over TN5250. Each must
//! open its own panel; text left on the menu's message lines is a shim.

use tn5250_driver::script::Context;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// The screen as text, one row a line.
fn screen(d: &tn5250_driver::session::Session) -> String {
    let mut out = String::new();
    for row in d.term.text_rows() {
        out.push('|');
        out.push_str(&row);
        out.push_str("|\n");
    }
    out
}

impl Suite {
    /// Step 82: `WRKHDWRSC` and `GO USER` option 2 from the main menu.
    pub(crate) fn display_commands(&self, _s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions {
            golden: None,
            update: false,
            release: String::new(),
            timeout_ms: 20000,
        };
        let mut ctx = Context::new(&c, &options);
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        secofr(&mut d, &mut ctx)?;
        run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
        let mut report = String::new();
        for (cmd, keys) in [
            ("WRKHDWRSC TYPE(*CMN)", ""),
            ("WRKHDWRSC TYPE(*STG)", ""),
            ("GO USER", "type 2\nkey enter\nsettle 1500\n"),
        ] {
            run(&mut d, &mut ctx, &format!("type \"{cmd}\"\nkey enter\nsettle 1500\n{keys}"))?;
            report.push_str(&format!("--- {cmd}\n{}", screen(&d)));
            for _ in 0..3 {
                if d.term.contains("Main Menu") {
                    break;
                }
                run(&mut d, &mut ctx, "key f3\nsettle 800\n")?;
            }
        }
        d.disconnect();
        Ok(report)
    }
}
