// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 66: the network tools. `PING` reaches the guest's own address,
//! QEMU's gateway and `LOOPBACK` by name from the host table;
//! `TRACEROUTE` finds the gateway in one hop; `NETSTAT` lists the
//! interfaces, routes and the telnet listener as text; a communications
//! trace of the NIC's link catches the pings and prints them.

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The guest's address on QEMU's user network.
const GUEST: &str = "10.0.2.15";
/// QEMU's gateway.
const GATEWAY: &str = "10.0.2.2";

/// A console command whose output holds every one of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    match want.iter().find(|w| !out.contains(*w)) {
        None => Ok(out),
        Some(w) => Err(format!("{line}: no {w} in:\n{out}")),
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 66: `PING`, `TRACEROUTE`, `NETSTAT` and a communications trace.
    pub(crate) fn net_tools(&self, s: &mut Session) -> Result<String, String> {
        expect(
            s,
            &format!("PING RMTSYS('{GUEST}') NBRPKT(2)"),
            &["TCP3204", "TCP3215", "TCP3211", "TCP3210", "2 of 2"],
        )?;
        expect(s, "PING RMTSYS(LOOPBACK) NBRPKT(1)", &["TCP3203", "127.0.0.1", "1 of 1"])?;
        expect(s, "PING RMTSYS(*INTNETADR) INTNETADR('::1') NBRPKT(1)", &["TCP3217", "1 of 1"])?;
        let out = expect(s, "PING RMTSYS(NO.SUCH.HOST.QS400.) NBRPKT(1)", &["TCP3202"])?;
        let _ = out;
        // The trace runs while the gateway is pinged.
        expect(s, "STRCMNTRC CFGOBJ(ETH0) CFGTYPE(*LIN) TEXT('Step 66')", &["CPF39A0"])?;
        expect(
            s,
            &format!("PING RMTSYS('{GATEWAY}') NBRPKT(3) WAITTIME(2)"),
            &["TCP3215", "TCP3210"],
        )?;
        expect(s, "CHKCMNTRC", &["CPF39A5", "ETH0", "Active", "CPF39A4"])?;
        expect(
            s,
            &format!("TRACEROUTE RMTSYS('{GATEWAY}') RANGE(1 3) PROBES(2) WAITTIME(2)"),
            &["TCP3260", "TCP3261", GATEWAY],
        )?;
        expect(s, "ENDCMNTRC CFGOBJ(ETH0) CFGTYPE(*LIN)", &["CPF39A1"])?;
        expect(s, "PRTCMNTRC CFGOBJ(ETH0) CFGTYPE(*LIN)", &["CPF39A3"])?;
        let listing = expect(
            s,
            "dspsplf QSYSPRT JOB(QSVCJOB) SPLNBR(*LAST)",
            &["Communications trace of ETH0", "ICMP", GATEWAY],
        )?;
        let records = listing.lines().filter(|l| l.contains("ETHV2")).count();
        expect(s, "DLTCMNTRC CFGOBJ(ETH0) CFGTYPE(*LIN)", &["CPF39A2"])?;
        // NETSTAT without a display: the views as text.
        expect(s, "NETSTAT OPTION(*IFC)", &["127.0.0.1", GUEST, "Active"])?;
        // The IPL gives the guest its address and no default route.
        expect(s, "NETSTAT OPTION(*RTE)", &["10.0.2.0", "255.255.255.0", "*DIRECT", "127.0.0.0"])?;
        expect(s, "NETSTAT OPTION(*CNN)", &["Listen", "23"])?;
        expect(s, "NETSTAT OPTION(*IFC6)", &["::1"])?;
        expect(s, "NETSTAT OPTION(*STATUS)", &["IPv4 stack status"])?;
        Ok(format!(
            "PING {GUEST}, LOOPBACK, ::1 and {GATEWAY}; TRACEROUTE to {GATEWAY}; NETSTAT *IFC *RTE *CNN *IFC6 *STATUS; trace of eth0 printed with {records} frames"
        ))
    }
}
