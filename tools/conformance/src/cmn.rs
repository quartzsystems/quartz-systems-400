// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step: an Ethernet line over the guest's virtio NIC,
//! varied on and off, its status following the cable QMP pulls.

use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The line the step makes.
const LINE: &str = "QETHLIN";

/// The QEMU network device of the guest's NIC (`-netdev ...,id=n0`).
#[cfg(unix)]
const NETDEV: &str = "n0";

/// The `CMNnn` resource of the virtio Ethernet port, from `hw`.
fn virtio_port(s: &mut Session) -> Result<String, String> {
    let out = s.command("hw", CMD)?;
    out.lines()
        .find(|l| l.starts_with("CMN") && l.contains("Ethernet port"))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
        .ok_or_else(|| format!("hw shows no Ethernet port:\n{out}"))
}

/// The status `WRKCFGSTS CFGTYPE(*LIN)` gives `name` (the first word
/// after it on its row).
fn status_of(out: &str, name: &str) -> Option<String> {
    out.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        (w.next()? == name).then(|| w.collect::<Vec<_>>().join(" "))
    })
}

/// Poll `WRKCFGSTS` until the line's row starts with one of `want`.
fn wait_status(s: &mut Session, want: &[&str], timeout: Duration) -> Result<String, String> {
    let until = Instant::now() + timeout;
    loop {
        let out = s.command("WRKCFGSTS CFGTYPE(*LIN)", CMD)?;
        if let Some(st) = status_of(&out, LINE)
            && want.iter().any(|w| st.starts_with(w))
        {
            return Ok(out);
        }
        if Instant::now() >= until {
            return Err(format!("{LINE} never became {want:?}:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Poll the operator's queue until a message holds `needle`.
fn wait_message(s: &mut Session, needle: &str, timeout: Duration) -> Result<(), String> {
    let until = Instant::now() + timeout;
    loop {
        let out = s.command("DSPMSG MSGQ(QSYSOPR)", CMD)?;
        if out.contains(needle) {
            return Ok(());
        }
        if Instant::now() >= until {
            return Err(format!("QSYSOPR has no message with {needle:?}:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
fn set_link(suite: &Suite, up: bool) -> Result<(), String> {
    crate::qemu::qmp::Qmp::connect(&suite.launch.qmp)?.set_link(NETDEV, up)
}

#[cfg(not(unix))]
fn set_link(_suite: &Suite, _up: bool) -> Result<(), String> {
    Err("QMP needs a Unix host".into())
}

/// Every line of `out` that names a message identifier from `ids`
/// must be there.
fn expect_ids(out: &str, ids: &[&str], what: &str) -> Result<(), String> {
    for id in ids {
        if !out.contains(id) {
            return Err(format!("{what}: no {id} in:\n{out}"));
        }
    }
    Ok(())
}

impl Suite {
    /// Step 60: `CRTLINETH` over the virtio port, `VRYCFG *ON` makes the
    /// network controller and device and the line `ACTIVE`; QMP takes
    /// the link down: `RCYPND` and the operator's message; up again:
    /// `ACTIVE`; `VRYCFG *OFF`: `VARIED OFF`; the line deleted.
    pub(crate) fn cmn_lines(&self, s: &mut Session) -> Result<String, String> {
        let port = virtio_port(s)?;
        let out = s.command(
            &format!(
                "CRTLINETH LIND({LINE}) RSRCNAME({port}) ONLINE(*NO) CMNRCYLMT(2 5) TEXT('Harness line')"
            ),
            CMD,
        )?;
        expect_ids(&out, &["CPC2601"], "CRTLINETH")?;
        let out = s.command("DSPLIND LIND(QETHLIN)", CMD)?;
        if !out.contains(&port) {
            return Err(format!("DSPLIND does not show {port}:\n{out}"));
        }
        let out = s.command(
            &format!("VRYCFG CFGOBJ({LINE}) CFGTYPE(*LIN) STATUS(*ON) VRYWAIT(15)"),
            Duration::from_secs(30),
        )?;
        expect_ids(&out, &["CPC2629", "CPC2630", "CPC2607"], "VRYCFG *ON")?;
        let out = wait_status(s, &["ACTIVE"], Duration::from_secs(20))?;
        let net: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("  ") && l.contains("ACTIVE"))
            .collect();
        if net.len() != 2 {
            return Err(format!("controller and device are not ACTIVE under {LINE}:\n{out}"));
        }
        set_link(self, false)?;
        wait_status(s, &["RCYPND"], Duration::from_secs(20))?;
        wait_message(s, "recovering", Duration::from_secs(20))?;
        set_link(self, true)?;
        wait_status(s, &["ACTIVE"], Duration::from_secs(20))?;
        let out = s.command(
            &format!("VRYCFG CFGOBJ({LINE}) CFGTYPE(*LIN) STATUS(*OFF)"),
            Duration::from_secs(30),
        )?;
        expect_ids(&out, &["CPC2608"], "VRYCFG *OFF")?;
        let out = wait_status(s, &["VARIED OFF"], Duration::from_secs(10))?;
        let off = out.lines().filter(|l| l.contains("VARIED OFF")).count();
        let out = s.command(&format!("DLTLIND LIND({LINE})"), CMD)?;
        expect_ids(&out, &["CPC2604"], "DLTLIND")?;
        Ok(format!(
            "{LINE} on {port}: ACTIVE, RCYPND with the cable out, ACTIVE again, {off} row(s) VARIED OFF, deleted"
        ))
    }
}
