// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 159: hardware resource names last across IPLs, in a guest of its
//! own. With two virtio NICs a line goes on the second one's `CMNnn` and
//! TCP/IP runs over it; at the next IPL a third NIC sits at a lower PCI
//! address than both, where the old walk order would have renumbered
//! them. The two keep their names, the new one takes the next free
//! `CMN` name, and the line is active on the same port again.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::qemu::{Launch, Nic};
use crate::steps::{CMD, StepReport, Suite};

const NAME: &str = "159 resource names across IPLs";
const LINE: &str = "QHWNLIN";
const ADDR: &str = "10.0.3.15";
/// QEMU's MAC for the first NIC, the telnet one (`n0`).
const MAC0: &str = "52:54:00:12:34:56";
const MAC1: &str = "52:54:00:15:90:01";
const MAC2: &str = "52:54:00:15:90:02";

fn nic(id: &str, slot: u8, mac: &str, net: &str) -> Nic {
    Nic {
        id: id.into(),
        slot,
        mac: mac.into(),
        net: net.into(),
    }
}

/// The `CMNnn` of the port with `mac`, and its interface, from `hw`.
fn port_of(hw: &str, mac: &str) -> Option<(String, String)> {
    hw.lines()
        .filter(|l| l.starts_with("CMN") && l.contains(mac))
        .find_map(|l| {
            let name = l.split_whitespace().next()?.to_string();
            // "Virtio Ethernet port eth1 52:54:..."
            let ifname = l
                .split_whitespace()
                .skip_while(|w| *w != "port")
                .nth(1)?
                .to_string();
            Some((name, ifname))
        })
}

/// Every `CMN` number `hw` shows, whatever its status.
fn cmn_numbers(hw: &str) -> Vec<u32> {
    hw.lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter_map(|n| n.strip_prefix("CMN")?.parse().ok())
        .collect()
}

/// `hw` until it lists the port with `mac`.
fn hw_with(s: &mut Session, mac: &str) -> Result<String, String> {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let hw = s.command("hw", CMD)?;
        if hw.contains("Resource  Type") && port_of(&hw, mac).is_some() {
            return Ok(hw);
        }
        if Instant::now() > until {
            return Err(format!("hw shows no port {mac}:\n{hw}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// A console command whose output holds `want`. A long answer before it
/// (`hw`) can still be on the line: then the stream is read on to it.
fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, Duration::from_secs(30))?;
    if out.contains(want) {
        return Ok(out);
    }
    match s.expect(want, Duration::from_secs(30)) {
        Ok(more) => Ok(format!("{out}{more}{want}")),
        Err(_) => Err(format!("{line}: no {want} in:\n{out}")),
    }
}

/// Wait until the interface is active on the line and the kernel has the
/// address on `ifname`.
fn line_active(s: &mut Session, ifname: &str) -> Result<String, String> {
    let ifc = format!("ifc {ADDR} /24 {LINE} Active");
    let addr = format!("kernel addr {ifname} {ADDR}/24");
    let until = Instant::now() + Duration::from_secs(180);
    loop {
        let out = s.command("svc.tcp ifc", CMD)?;
        if out.contains("TCP/IP Active") && out.contains(&ifc) && out.contains(&addr) {
            return Ok(out);
        }
        if Instant::now() > until {
            return Err(format!("no {ifc:?} with {addr:?}:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// The tail of a session's serial log, for a failure.
fn tail(s: &Session) -> String {
    let lines: Vec<&str> = s.log.lines().collect();
    lines[lines.len().saturating_sub(60)..].join("\n")
}

impl Suite {
    /// Step 159 in its own guest (`--only 159` runs just it).
    pub(crate) fn hw_name_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !only.iter().any(|p| NAME.starts_with(p.as_str()))
        {
            return Ok(Vec::new());
        }
        let mut s = Session::spawn(&self.two_nics())?;
        let r = self.step(&mut s, NAME, Self::resource_names)?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        Ok(vec![r])
    }

    /// The telnet NIC and a second one, both above slot 0x11.
    fn two_nics(&self) -> Launch {
        Launch {
            nic_slot: Some(0x12),
            nics: vec![nic("n1", 0x13, MAC1, "10.0.3.0/24")],
            ..self.launch.clone()
        }
    }

    #[allow(clippy::too_many_lines)] // two IPLs, one walk
    fn resource_names(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::steps::SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect("Service console ready", 4 * self.boot_timeout)?;
        let mut report = String::new();
        let hw = hw_with(s, MAC1)?;
        let (cmn0, _) = port_of(&hw, MAC0).ok_or_else(|| format!("no port {MAC0}:\n{hw}"))?;
        let (cmn1, if1) = port_of(&hw, MAC1).ok_or_else(|| format!("no port {MAC1}:\n{hw}"))?;
        let next = cmn_numbers(&hw).into_iter().max().unwrap_or(0) + 1;
        let _ = write!(report, "--- first IPL\n{hw}");

        // A line on the second port, TCP/IP over it.
        expect(
            s,
            &format!(
                "CRTLINETH LIND({LINE}) RSRCNAME({cmn1}) ONLINE(*YES) TEXT('Names across IPLs')"
            ),
            "CPC2601",
        )?;
        expect(
            s,
            &format!(
                "ADDTCPIFC INTNETADR('{ADDR}') LIND({LINE}) SUBNETMASK('255.255.255.0') AUTOSTART(*YES)"
            ),
            "TCP1D03",
        )?;
        expect(
            s,
            &format!("VRYCFG CFGOBJ({LINE}) CFGTYPE(*LIN) STATUS(*ON) VRYWAIT(15)"),
            "CPC2607",
        )?;
        expect(s, &format!("STRTCPIFC INTNETADR('{ADDR}')"), "TCP1A0B")?;
        let ifc = line_active(s, &if1)?;
        let _ = write!(report, "--- {LINE} on {cmn1} ({if1})\n{ifc}\n");
        s.command("pwrdwn", CMD).ok();
        if !s.wait_exit(Duration::from_secs(60)) {
            return Err(format!("the first IPL did not end:\n{}", tail(s)));
        }

        // The next IPL with a third NIC below both.
        let mut three = self.two_nics();
        three.nics.push(nic("n2", 0x11, MAC2, "10.0.4.0/24"));
        let mut t = Session::spawn(&three).map_err(|e| e.to_string())?;
        let r = self.after_new_card(&mut t, &cmn0, &cmn1, next, &mut report);
        t.command("pwrdwn", CMD).ok();
        t.wait_exit(Duration::from_secs(20));
        std::fs::write(self.out_dir.join("step159.txt"), &report).map_err(|e| e.to_string())?;
        r.map_err(|e| format!("{e}\n--- second IPL's serial log\n{}", tail(&t)))
    }

    /// The second IPL's checks, then the line and interface removed.
    fn after_new_card(
        &self,
        t: &mut Session,
        cmn0: &str,
        cmn1: &str,
        next: u32,
        report: &mut String,
    ) -> Result<String, String> {
        t.expect(crate::steps::SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        t.expect("Service console ready", 4 * self.boot_timeout)?;
        let hw = hw_with(t, MAC2)?;
        let _ = write!(report, "--- second IPL\n{hw}");
        let now0 = port_of(&hw, MAC0).map(|p| p.0);
        let now1 = port_of(&hw, MAC1);
        let new = port_of(&hw, MAC2).map(|p| p.0);
        if now0.as_deref() != Some(cmn0) || now1.as_ref().map(|p| p.0.as_str()) != Some(cmn1) {
            return Err(format!(
                "the ports were {cmn0} and {cmn1}, now {now0:?} and {:?}:\n{hw}",
                now1.map(|p| p.0)
            ));
        }
        let want = format!("CMN{next:02}");
        if new.as_deref() != Some(want.as_str()) {
            return Err(format!("the new port is {new:?}, not {want}:\n{hw}"));
        }
        let (_, if1) = now1.expect("checked");
        let out = expect(t, &format!("DSPLIND LIND({LINE})"), cmn1)?;
        let _ = write!(report, "--- DSPLIND\n{out}\n");
        let ifc = line_active(t, &if1)?;
        let _ = write!(report, "--- {LINE} on {cmn1} ({if1}) again\n{ifc}\n");
        // Leave the disk as it was for the steps after.
        expect(t, &format!("ENDTCPIFC INTNETADR('{ADDR}')"), "TCP")?;
        expect(t, &format!("RMVTCPIFC INTNETADR('{ADDR}')"), "TCP")?;
        expect(
            t,
            &format!("VRYCFG CFGOBJ({LINE}) CFGTYPE(*LIN) STATUS(*OFF)"),
            "CPC2608",
        )?;
        expect(t, &format!("DLTLIND LIND({LINE})"), "CPC2604")?;
        Ok(format!(
            "{cmn0} and {cmn1} kept with {want} new below them; {LINE} active on {cmn1} ({if1}) after the IPL"
        ))
    }
}
