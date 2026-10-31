// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 83: hardware resources over TN5250, in a guest of its own with
//! an empty virtio SCSI controller. `WRKHDWRSC TYPE(*CMN)` shows the
//! virtio NIC's port as `CMN01` with its type, `TYPE(*STG)` the virtio
//! disk controller and `DD001`; a disk hot-added through QMP shows in
//! the list after F5, with no IPL, and in `svc.hw`.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use tn5250_driver::script::Context;
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, StepReport, Suite};

const NAME: &str = "83 hardware resources";

/// The screen as text, one row a line.
fn screen(d: &Display) -> String {
    d.term.text_rows().iter().fold(String::new(), |mut s, r| {
        let _ = writeln!(s, "|{r}|");
        s
    })
}

/// The resource names a list shows that start with `prefix`.
fn names(d: &Display, prefix: &str) -> Vec<String> {
    d.term
        .text_rows()
        .iter()
        .skip(6)
        .filter_map(|r| r.get(6..22).map(str::trim).map(str::to_string))
        .filter(|n| n.starts_with(prefix) && n[prefix.len()..].chars().all(|c| c.is_ascii_digit()))
        .collect()
}

/// The row of resource `name`, if the list shows it.
fn row_of(d: &Display, name: &str) -> Option<String> {
    d.term
        .text_rows()
        .into_iter()
        .find(|r| r.get(6..22).is_some_and(|c| c.trim() == name))
}

impl Suite {
    /// Step 83 in its own guest (`--only 83` runs just it).
    pub(crate) fn hw_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !only.iter().any(|p| NAME.starts_with(p.as_str()))
        {
            return Ok(Vec::new());
        }
        let launch = Launch {
            scsi: true,
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let r = self.step(&mut s, NAME, Self::hardware_resources)?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        Ok(vec![r])
    }

    #[allow(clippy::too_many_lines)] // one walk through the displays
    fn hardware_resources(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::steps::SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect("Service console ready", 4 * self.boot_timeout)?;
        // The password set here, so the step needs no earlier one.
        let set = s.command(
            &format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"),
            self.boot_timeout,
        )?;
        if set.trim_start().starts_with("CPF") {
            return Err(format!("CHGUSRPRF QSECOFR: {set}"));
        }
        // A new guest: the main session's forward to the address step 61
        // configured is gone, so make it again; the launch's own forward
        // (the guest's first address) when step 61 did not run.
        let port = if self.tn_port.get().is_some() {
            crate::tcp::forward_port(self, crate::tcp::VIP, 23)?
        } else {
            self.launch.telnet_port.ok_or("the launch has no telnet port")?
        };
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

        // Communications: the virtio NIC's port with its type.
        run(&mut d, &mut ctx, "type \"WRKHDWRSC TYPE(*CMN)\"\nkey enter\nsettle 2500\n")?;
        let cmn = screen(&d);
        if !d.term.contains("Work with Communication Resources") {
            return Err(format!("WRKHDWRSC TYPE(*CMN) shows no list:\n{cmn}"));
        }
        let row = row_of(&d, "CMN01").ok_or_else(|| format!("no CMN01:\n{cmn}"))?;
        if !row.contains("QN01") || !row.contains("Operational") {
            return Err(format!("CMN01 is not the operational virtio port: {row}\n{cmn}"));
        }
        if !names(&d, "LIN").contains(&"LIN01".to_string()) {
            return Err(format!("no LIN01 above CMN01:\n{cmn}"));
        }
        // Option 7: the port's detail with its adapter address.
        run(&mut d, &mut ctx, "pick \"CMN01\" 7\nkey enter\nsettle 1500\n")?;
        let detail = screen(&d);
        if !d.term.contains("Display Resource Detail") || !d.term.contains("CMN01") {
            return Err(format!("option 7 on CMN01:\n{detail}"));
        }
        // The port's lines follow the addresses: page until they show.
        let mut detail2 = String::new();
        for _ in 0..4 {
            if detail2.contains("Adapter address") || d.term.contains("Bottom") {
                break;
            }
            run(&mut d, &mut ctx, "key pgdn\nsettle 1000\n")?;
            detail2.push_str(&screen(&d));
        }
        if !detail2.contains("Adapter address") {
            return Err(format!("CMN01's detail has no adapter address:\n{detail}{detail2}"));
        }
        let _ = write!(report, "--- *CMN\n{cmn}--- CMN01 detail\n{detail}{detail2}");
        run(&mut d, &mut ctx, "key f12\nsettle 1000\nkey f3\nsettle 1500\n")?;

        // Storage: the virtio disk controller, DD001, the SCSI controller.
        run(&mut d, &mut ctx, "type \"WRKHDWRSC TYPE(*STG)\"\nkey enter\nsettle 2500\n")?;
        let stg = screen(&d);
        if !d.term.contains("Work with Storage Resources") {
            return Err(format!("WRKHDWRSC TYPE(*STG) shows no list:\n{stg}"));
        }
        let dc01 = row_of(&d, "DC01").ok_or_else(|| format!("no DC01:\n{stg}"))?;
        if !dc01.contains("QD01") {
            return Err(format!("DC01 is not the virtio disk controller: {dc01}\n{stg}"));
        }
        if row_of(&d, "DD001").is_none() {
            return Err(format!("no DD001:\n{stg}"));
        }
        if !stg.contains("QD02") {
            return Err(format!("the virtio SCSI controller is not listed:\n{stg}"));
        }
        let before = names(&d, "DD");
        let _ = write!(report, "--- *STG\n{stg}");

        // Hot-add a disk: it shows after F5 with no IPL.
        let file = self.out_dir.join("hot0.img");
        std::fs::File::create(&file)
            .and_then(|f| f.set_len(64 << 20))
            .map_err(|e| format!("{}: {e}", file.display()))?;
        crate::qemu::qmp::Qmp::connect(&self.launch.qmp)?.hot_add_scsi_disk(&file, "hotdisk0")?;
        let until = Instant::now() + Duration::from_secs(60);
        let added = loop {
            run(&mut d, &mut ctx, "key f5\nsettle 2000\n")?;
            let now = names(&d, "DD");
            if let Some(n) = now.iter().find(|n| !before.contains(n)) {
                break n.clone();
            }
            if Instant::now() > until {
                return Err(format!("no new disk unit after the hot add:\n{}", screen(&d)));
            }
        };
        let after = screen(&d);
        let row = row_of(&d, &added).unwrap_or_default();
        if !row.contains("Operational") {
            return Err(format!("{added} is not operational: {row}\n{after}"));
        }
        let _ = write!(report, "--- *STG after the hot add ({added})\n{after}");
        run(&mut d, &mut ctx, "key f3\nsettle 1000\n")?;
        d.disconnect();

        // The service console reads the same table.
        // An earlier answer may still be on the line: ask until the
        // table comes back.
        let mut hw = String::new();
        for _ in 0..3 {
            hw = s.command("hw", CMD)?;
            if hw.contains("Resource  Type") {
                break;
            }
        }
        if !hw.contains(&added) {
            return Err(format!("svc.hw does not show {added}:\n{hw}"));
        }
        std::fs::write(self.out_dir.join("step83.txt"), &report).map_err(|e| e.to_string())?;
        Ok(format!("CMN01 QN01, DC01 QD01, DD001; {added} hot-added and listed"))
    }
}
