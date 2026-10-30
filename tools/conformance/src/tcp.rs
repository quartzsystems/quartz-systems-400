// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU steps: TCP/IP configuration applied to the kernel.
//! Step 61 adds interfaces, a route, attributes, a host table entry and
//! a port restriction while TCP/IP runs (`QSTRUP` started it), checks
//! what the kernel has through `svc.tcp`, and forwards a host port to
//! the configured address, which the TN5250 steps then use. Step 62
//! ends TCP/IP and starts it again.

use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The address step 61 configures as a virtual IP on the guest; QEMU's
/// user network reaches it as it reaches the guest's own address.
const VIP: &str = "10.0.2.16";
/// The address it puts on the NIC's line.
const LINE_ADDR: &str = "10.0.2.17";

/// A console command whose output must hold `want`.
fn expect(s: &mut Session, line: &str, want: &str) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    if out.contains(want) {
        Ok(out)
    } else {
        Err(format!("{line}: no {want} in:\n{out}"))
    }
}

/// Wait until `svc.tcp what` shows `want`.
fn until(s: &mut Session, what: &str, want: &str) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let out = s.command(&format!("svc.tcp {what}"), CMD)?;
        if out.contains(want) {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("svc.tcp {what}: no {want} in:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The telnet server answers on `port` (it negotiates first: IAC).
pub(crate) fn telnet_answers(port: u16) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(30);
    let mut last = String::new();
    while Instant::now() < end {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(mut c) => {
                let _ = c.set_read_timeout(Some(Duration::from_secs(10)));
                let mut b = [0u8; 3];
                match c.read(&mut b) {
                    Ok(n) if n > 0 && b[0] == 0xff => return Ok(()),
                    Ok(n) => last = format!("read {n} bytes: {:02x?}", &b[..n]),
                    Err(e) => last = e.to_string(),
                }
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(format!("telnet on host port {port}: {last}"))
}

/// The line the NIC's interface names: a line description when the
/// communications objects are there (`CRTLINETH`), else the kernel
/// link's name, which TCP/IP takes as a line until they are.
fn nic_line(s: &mut Session) -> Result<String, String> {
    let out = s.command("CRTLINETH LIND(ETHLINE) RSRCNAME(CMN01)", CMD)?;
    if out.contains("CPD0030") || out.contains("not found") && out.contains("CRTLINETH") {
        // BLOCKED(phase10): no *LIND objects in this tree yet (net/cmncfg).
        return Ok("ETH0".into());
    }
    Ok("ETHLINE".into())
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 61: configuration while TCP/IP is active.
    pub(crate) fn tcp_config(&self, s: &mut Session) -> Result<String, String> {
        until(s, "ifc", "TCP/IP Active")?;
        until(s, "ifc", "ifc 127.0.0.1 /8 *LOOPBACK Active")?;
        let jobs = s.command("wrkactjob", CMD)?;
        for name in ["QTCPCTL ", "QTCPWRK "] {
            if !jobs.lines().any(|l| l.trim_start().starts_with(name) && l.contains("SYS")) {
                return Err(format!("no system job {name}:\n{jobs}"));
            }
        }
        let line = nic_line(s)?;
        expect(
            s,
            &format!("ADDTCPIFC INTNETADR('{VIP}') LIND(*VIRTUALIP) SUBNETMASK('255.255.255.255')"),
            "TCP1D03",
        )?;
        expect(
            s,
            &format!("ADDTCPIFC INTNETADR('{LINE_ADDR}') LIND({line}) SUBNETMASK('255.255.255.0') AUTOSTART(*NO)"),
            "TCP1D03",
        )?;
        expect(s, &format!("STRTCPIFC INTNETADR('{VIP}')"), "TCP1A0B")?;
        expect(s, &format!("STRTCPIFC INTNETADR('{LINE_ADDR}')"), "TCP1A0B")?;
        until(s, "ifc", &format!("kernel addr dummy0 {VIP}/32"))?;
        until(s, "ifc", &format!("kernel addr eth0 {LINE_ADDR}/24"))?;
        expect(
            s,
            "ADDTCPRTE RTEDEST('192.168.77.0') SUBNETMASK('255.255.255.0') NEXTHOP('10.0.2.2')",
            "TCP1D03",
        )?;
        until(s, "rte", "kernel route 192.168.77.0/24 via 10.0.2.2 dev eth0 qs400")?;
        expect(s, "CHGTCPA TCPKEEPALV(10) IPTTL(60)", "TCP1D09")?;
        until(s, "attr", "ipv4/tcp_keepalive_time want 600 kernel 600")?;
        until(s, "attr", "ipv4/ip_default_ttl want 60 kernel 60")?;
        expect(s, &format!("ADDTCPHTE INTNETADR('{VIP}') HOSTNAME((QS400VIP))"), "TCP1D03")?;
        until(s, "host", &format!("host {VIP} QS400VIP"))?;
        expect(s, "ADDTCPPORT PORT(8000 8100) PROTOCOL(*TCP) USRPRF(QTCP)", "TCP1D03")?;
        until(s, "port", "kernel port rules 1")?;
        // The interface and route of the NIC's line go again.
        expect(s, "RMVTCPRTE RTEDEST('192.168.77.0') SUBNETMASK('255.255.255.0') NEXTHOP('10.0.2.2')", "TCP1D05")?;
        expect(s, &format!("ENDTCPIFC INTNETADR('{LINE_ADDR}')"), "TCP1A0C")?;
        let ifc = s.command("svc.tcp ifc", CMD)?;
        if ifc.contains(&format!("{LINE_ADDR}/24")) {
            return Err(format!("{LINE_ADDR} is still on eth0:\n{ifc}"));
        }
        expect(s, &format!("RMVTCPIFC INTNETADR('{LINE_ADDR}')"), "TCP1D05")?;
        // The TN5250 steps now come in over the virtual address.
        let port = forward(self, VIP)?;
        if let Err(e) = telnet_answers(port) {
            // Tell slirp trouble from the server's: the guest's own address.
            let own = forward(self, "10.0.2.15")?;
            let theirs = telnet_answers(own).err().unwrap_or_else(|| "answers".into());
            let ifc = s.command("svc.tcp ifc", CMD)?;
            return Err(format!("{e}; over 10.0.2.15: {theirs}
{ifc}"));
        }
        self.tn_port.set(Some(port));
        Ok(format!(
            "QTCPCTL and QTCPWRK run; {VIP} on dummy0 and {LINE_ADDR} on {line}; route, CHGTCPA, host table and port rule in the kernel; telnet over {VIP} on host port {port}"
        ))
    }

    /// Step 62: `ENDTCP` takes the configured addresses and the telnet
    /// server down; `STRTCP` brings both back.
    pub(crate) fn tcp_restart(&self, s: &mut Session) -> Result<String, String> {
        expect(s, "ENDTCP", "TCP1A02")?;
        until(s, "ifc", "TCP/IP Inactive")?;
        let ifc = s.command("svc.tcp ifc", CMD)?;
        if ifc.contains(&format!("{VIP}/32")) {
            return Err(format!("ENDTCP left {VIP}:\n{ifc}"));
        }
        expect(s, "STRTCP", "TCP1A00")?;
        until(s, "ifc", &format!("kernel addr dummy0 {VIP}/32"))?;
        if let Some(port) = self.tn_port.get() {
            telnet_answers(port)?;
        }
        Ok(format!("ENDTCP ended {VIP} and telnet; STRTCP restarted both"))
    }
}

/// Forward a fresh host port to the guest's `addr`:23.
fn forward(suite: &Suite, addr: &str) -> Result<u16, String> {
    forward_port(suite, addr, 23)
}

/// Forward a fresh host port to the guest's `addr`:`guest`.
#[cfg(unix)]
pub(crate) fn forward_port(suite: &Suite, addr: &str, guest: u16) -> Result<u16, String> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())?;
    let mut q = crate::qemu::qmp::Qmp::connect(&suite.launch().qmp)?;
    let out = q.hmp(&format!("hostfwd_add n0 tcp:127.0.0.1:{port}-{addr}:{guest}"))?;
    if !out.trim().is_empty() {
        return Err(format!("hostfwd_add: {out}"));
    }
    Ok(port)
}

#[cfg(not(unix))]
pub(crate) fn forward_port(_: &Suite, _: &str, _: u16) -> Result<u16, String> {
    Err("QMP needs a Unix host".into())
}
