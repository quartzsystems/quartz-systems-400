// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 179 to 184 in a pair of guests of their own: A boots the
//! suite's disk, B a copy of it, each with a second NIC on a private
//! link between them (`qemu::Link`: their frames over UDP on the host's
//! loopback). On that link A is `192.168.77.1` (`guesta.nfs.test`) and B
//! `192.168.77.2` (`guestb.nfs.test`), each in the other's host table, so
//! the protocols run as between two machines: B asks A's binder on 111,
//! A's status monitor notifies B's at a restart, B's client reclaims.
//! QEMU's user network cannot carry that (the binder and the lock
//! manager's callbacks need real ports both ways; `docs/NFS.md` 7.7 has
//! no option naming the lock manager's port).
//!
//! The setup (not a step of its own) runs when any of the steps is
//! chosen: the link as a line description (`CRTLINETH` over the NIC's
//! `CMNnn`, as step 159 does), the names, `QBATCH`, the daemons on both,
//! and on A the exports: `/nfstest` (root squashed), `/nfsroot`
//! (`ROOT=192.168.77.2`) and `/QSYS.LIB/NFSLIB.LIB` (`ROOT=`, `HOSTOPT`
//! data CCSID 819 for B).

use std::time::{Duration, Instant};

use super::{CMD, clean, export, full_log, public_dir, start_servers, want};
use crate::HarnessError;
use crate::qemu::session::Session;
use crate::qemu::{Launch, Link};
use crate::steps::{StepReport, Suite, free_port};

/// The guests' ends of the link.
pub(crate) const A_ADDR: &str = "192.168.77.1";
pub(crate) const B_ADDR: &str = "192.168.77.2";
/// Their names (`CHGTCPDMN`), lower case as NLM and NSM send them.
pub(crate) const A_NAME: &str = "guesta.nfs.test";
pub(crate) const B_NAME: &str = "guestb.nfs.test";
/// The link's line description on both.
const LINE: &str = "NFSLNK";
/// The link NICs' MAC addresses.
const A_MAC: &str = "52:54:00:4e:46:01";
const B_MAC: &str = "52:54:00:4e:46:02";
/// The export B's root is root on.
pub(crate) const ROOTED: &str = "/nfsroot";
/// The library exported through `/QSYS.LIB`.
pub(crate) const LIB: &str = "NFSLIB";
/// The first line of its member `HELLO`.
pub(crate) const MEMBER_TEXT: &str = "NFSQSYSTEXT LINE ONE";

/// The two guests.
pub(crate) struct Pair {
    /// The exporting guest (the suite's disk).
    pub a: Session,
    /// The mounting guest (a copy).
    pub b: Session,
    /// When A's daemons last started (the grace periods run from it).
    pub a_started: Instant,
}

impl Pair {
    /// Wait until A's grace periods are over.
    pub fn after_grace(&self) {
        std::thread::sleep(super::AFTER_GRACE.saturating_sub(self.a_started.elapsed()));
    }
}

/// A step of the pair.
type PairStep = fn(&Suite, &mut Pair) -> Result<String, String>;

/// The steps, in order.
const STEPS: [(&str, PairStep); 6] = [
    ("179 NFS two guests version 3", Suite::nfs_pair_v3),
    (
        "180 NFS two guests version 4 and QSYS.LIB",
        Suite::nfs_pair_v4,
    ),
    ("181 NFS locks across guests", Suite::nfs_pair_locks),
    ("182 NFS lock recovery", Suite::nfs_pair_recovery),
    ("183 NFS soft and intr mounts", Suite::nfs_pair_soft),
    ("184 NFS copy between guests", Suite::nfs_pair_copy),
];

/// The one-line link descriptions of `hw` (`CMNnn ... port eth1 MAC`).
pub(crate) fn port_of(hw: &str, mac: &str) -> Option<(String, String)> {
    hw.lines()
        .filter(|l| l.starts_with("CMN") && l.to_ascii_lowercase().contains(mac))
        .find_map(|l| {
            let name = l.split_whitespace().next()?.to_string();
            let ifname = l
                .split_whitespace()
                .skip_while(|w| *w != "port")
                .nth(1)?
                .to_string();
            Some((name, ifname))
        })
}

/// The last lines of a session's serial log.
fn tail(s: &Session, n: usize) -> String {
    let lines: Vec<&str> = s.log.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// IPL complete and the service console's prompt.
fn booted(s: &mut Session, limit: Duration) -> Result<(), String> {
    s.expect(crate::steps::SRC_IPL_COMPLETE, limit)?;
    s.expect("Service console ready", limit)?;
    s.expect(crate::PROMPT, CMD)?;
    Ok(())
}

/// Wait until `svc.tcp what` shows every one of `wants`.
fn tcp_until(
    s: &mut Session,
    what: &str,
    wants: &[&str],
    limit: Duration,
) -> Result<String, String> {
    let end = Instant::now() + limit;
    loop {
        let out = s.command(&format!("svc.tcp {what}"), CMD)?;
        if wants.iter().all(|w| out.contains(w)) {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("svc.tcp {what}: not all of {wants:?} in:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// The link NIC as a line, the address on it, active.
fn link_up(s: &mut Session, mac: &str, addr: &str) -> Result<String, String> {
    tcp_until(s, "ifc", &["TCP/IP Active"], Duration::from_secs(180))?;
    let end = Instant::now() + Duration::from_secs(60);
    let (cmn, ifname) = loop {
        let hw = s.command("hw", Duration::from_secs(30))?;
        if let Some(p) = port_of(&hw, mac) {
            break p;
        }
        if Instant::now() > end {
            return Err(format!("hw shows no port {mac}:\n{hw}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    };
    let lind = s.command(&format!("DSPLIND LIND({LINE})"), CMD)?;
    if !lind.contains(&cmn) {
        want(
            s,
            &format!("CRTLINETH LIND({LINE}) RSRCNAME({cmn}) ONLINE(*YES) TEXT('NFS pair link')"),
            &["CPC2601"],
            Duration::from_secs(30),
        )?;
    }
    let ifc = s.command("svc.tcp ifc", CMD)?;
    if !ifc.lines().any(|l| l.starts_with(&format!("ifc {addr} "))) {
        want(
            s,
            &format!(
                "ADDTCPIFC INTNETADR('{addr}') LIND({LINE}) SUBNETMASK('255.255.255.0') AUTOSTART(*YES)"
            ),
            &["TCP1D03"],
            CMD,
        )?;
    }
    s.command(
        &format!("VRYCFG CFGOBJ({LINE}) CFGTYPE(*LIN) STATUS(*ON) VRYWAIT(15)"),
        Duration::from_secs(30),
    )?;
    let active = format!("ifc {addr} /24 {LINE} Active");
    if !s.command("svc.tcp ifc", CMD)?.contains(&active) {
        want(
            s,
            &format!("STRTCPIFC INTNETADR('{addr}')"),
            &["TCP1A0B"],
            CMD,
        )?;
    }
    tcp_until(
        s,
        "ifc",
        &[&active, &format!("kernel addr {ifname} {addr}/24")],
        Duration::from_secs(120),
    )?;
    Ok(format!("{addr} on {LINE} ({cmn}, {ifname})"))
}

/// This guest's name and both guests' in its host table.
fn names(s: &mut Session, me: &str) -> Result<(), String> {
    let (host, _) = me.split_once('.').unwrap_or((me, ""));
    want(
        s,
        &format!(
            "CHGTCPDMN HOSTNAME('{}') DMNNAME('NFS.TEST')",
            host.to_ascii_uppercase()
        ),
        &["TCP1D04"],
        CMD,
    )?;
    let table = s.command("svc.tcp host", CMD)?;
    for (addr, name) in [(A_ADDR, A_NAME), (B_ADDR, B_NAME)] {
        if table.contains(addr) {
            continue;
        }
        let short = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
        want(
            s,
            &format!(
                "ADDTCPHTE INTNETADR('{addr}') HOSTNAME(({}) ({short}))",
                name.to_ascii_uppercase()
            ),
            &["TCP1D03"],
            CMD,
        )?;
    }
    tcp_until(
        s,
        "host",
        &[
            A_ADDR,
            B_ADDR,
            &A_NAME.to_ascii_uppercase(),
            &B_NAME.to_ascii_uppercase(),
        ],
        Duration::from_secs(30),
    )?;
    Ok(())
}

/// `PING` across the link until it answers.
fn ping(s: &mut Session, addr: &str) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let out = s.command(
            &format!("PING RMTSYS('{addr}') NBRPKT(2) WAITTIME(2)"),
            Duration::from_secs(30),
        )?;
        if out.contains("2 of 2") || out.contains("1 of 2") {
            return Ok(out);
        }
        if Instant::now() > end {
            return Err(format!("PING {addr} across the link:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The source of member `HELLO`.
fn member_source() -> String {
    format!("{MEMBER_TEXT}\nSECOND LINE\n")
}

impl Suite {
    /// Steps 179 to 184 in their pair of guests (with `--only` when named).
    pub(crate) fn nfs_pair_steps(&self) -> Result<Vec<StepReport>, HarnessError> {
        let chosen: Vec<&(&str, PairStep)> = STEPS
            .iter()
            .filter(|(n, _)| {
                self.only
                    .as_ref()
                    .is_none_or(|p| p.iter().any(|p| n.starts_with(p.as_str())))
            })
            .collect();
        let Some(first) = chosen.first().map(|(n, _)| *n) else {
            return Ok(Vec::new());
        };
        let image = self
            .launch
            .image
            .clone()
            .ok_or_else(|| HarnessError::ImageMissing(self.out_dir.join("disk.path")))?;
        let copy = self.out_dir.join("nfs-b.img");
        std::fs::copy(&image, &copy)?;
        let fail = |reason: String| HarnessError::Failed {
            step: first,
            reason,
            log: String::new(),
        };
        let (pa, pb) = (
            super::host::free_udp().map_err(fail)?,
            super::host::free_udp().map_err(fail)?,
        );
        let dgram = std::env::var("QS400_NFS_LINK").is_ok_and(|v| v == "dgram");
        let link = |mac: &str, local, remote| Link {
            id: "l1".into(),
            slot: 0x14,
            mac: mac.into(),
            local,
            remote,
            dgram,
        };
        let launch_a = Launch {
            links: vec![link(A_MAC, pa, pb)],
            ..self.launch.clone()
        };
        let telnet = free_port();
        let launch_b = Launch {
            image: Some(copy),
            qmp: self.out_dir.join("qmp-nfsb.sock"),
            telnet_port: Some(telnet),
            host_ports: crate::hostsvr::forwards(Some(telnet)),
            links: vec![link(B_MAC, pb, pa)],
            ..self.launch.clone()
        };
        let mut p = Pair {
            a: Session::spawn(&launch_a)?,
            b: Session::spawn(&launch_b)?,
            a_started: Instant::now(),
        };
        let t = Instant::now();
        match self.nfs_pair_setup(&mut p) {
            Ok(d) => println!(
                "  PASS {:<24} {:6.1}s  {d}",
                "NFS pair setup",
                t.elapsed().as_secs_f64()
            ),
            Err(e) => return Err(self.pair_failed(&mut p, first, &format!("setup: {e}"))),
        }
        let mut reports = Vec::new();
        for (name, f) in chosen {
            let t = Instant::now();
            match f(self, &mut p) {
                Ok(detail) => {
                    println!(
                        "  PASS {name:<24} {:6.1}s  {detail}",
                        t.elapsed().as_secs_f64()
                    );
                    reports.push(StepReport {
                        name,
                        elapsed: t.elapsed(),
                        detail,
                    });
                }
                Err(e) => return Err(self.pair_failed(&mut p, name, &e)),
            }
        }
        for s in [&mut p.a, &mut p.b] {
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(30));
        }
        Ok(reports)
    }

    /// The failure of a pair step, both guests' state written out.
    fn pair_failed(&self, p: &mut Pair, step: &'static str, reason: &str) -> HarnessError {
        let num = step.split_whitespace().next().unwrap_or("pair");
        let da = super::diag::dump(&self.out_dir, num, "a", &mut p.a, "");
        let db = super::diag::dump(&self.out_dir, num, "b", &mut p.b, "");
        let err = HarnessError::Failed {
            step,
            reason: format!(
                "{reason}\n(guest state: {da}, {db})\n--- guest B's serial log, last lines:\n{}",
                tail(&p.b, 60)
            ),
            log: p.a.log.clone(),
        };
        for s in [&mut p.a, &mut p.b] {
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(20));
        }
        err
    }

    /// Both guests up, linked, named, serving; A's exports.
    fn nfs_pair_setup(&self, p: &mut Pair) -> Result<String, String> {
        let limit = 4 * self.boot_timeout;
        booted(&mut p.a, limit).map_err(|e| format!("guest A: {e}"))?;
        booted(&mut p.b, limit).map_err(|e| format!("guest B: {e}"))?;
        let mut links = Vec::new();
        for (s, mac, addr, name, who) in [
            (&mut p.a, A_MAC, A_ADDR, A_NAME, "A"),
            (&mut p.b, B_MAC, B_ADDR, B_NAME, "B"),
        ] {
            full_log(s)?;
            links.push(link_up(s, mac, addr).map_err(|e| format!("guest {who}: {e}"))?);
            names(s, name).map_err(|e| format!("guest {who}: {e}"))?;
            s.command("strsbs QSYS/QBATCH", CMD)?;
            s.command("svc.ifs mkdir /mnt", CMD)?;
        }
        ping(&mut p.a, B_ADDR)?;
        ping(&mut p.b, A_ADDR)?;
        // B's status monitor and lock manager first: a mount takes
        // whether they run (locks) when it is made.
        start_servers(&mut p.b, "*ALL").map_err(|e| format!("guest B: {e}"))?;
        public_dir(&mut p.a, super::EXPORT)?;
        public_dir(&mut p.a, ROOTED)?;
        export(&mut p.a, super::EXPORT, "RW", "")?;
        export(&mut p.a, ROOTED, &format!("RW,ROOT={B_ADDR}"), "")?;
        // The library: a source member to read as ASCII, one to write.
        let a = &mut p.a;
        a.command(&format!("CRTLIB LIB({LIB})"), CMD)?;
        a.command(&format!("CRTSRCPF FILE({LIB}/QCLSRC) RCDLEN(92)"), CMD)?;
        crate::qir::qemu::push(a, "NFSQSRC", &member_source())?;
        let put = a.command(
            &format!("svc.srcput {LIB}/QCLSRC HELLO SRCTYPE(CLLE) FROMSTMF(NFSQSRC)"),
            CMD,
        )?;
        if !put.contains("written") {
            return Err(format!("svc.srcput {LIB}/QCLSRC HELLO:\n{put}"));
        }
        a.command(&format!("ADDPFM FILE({LIB}/QCLSRC) MBR(FROMB)"), CMD)?;
        export(
            a,
            &format!("/QSYS.LIB/{LIB}.LIB"),
            &format!("RW,ROOT={B_ADDR}"),
            &format!("('{B_ADDR}' 819 *ASCII *SYNC)"),
        )?;
        let (started, running) = start_servers(a, "*ALL").map_err(|e| format!("guest A: {e}"))?;
        p.a_started = Instant::now();
        let table = super::sql(&mut p.a, "SELECT * FROM QSYS2/NFS_EXPORT_INFO")?;
        for want in [super::EXPORT, ROOTED, "819"] {
            if !table.contains(want) {
                return Err(format!("NFS_EXPORT_INFO lacks {want}:\n{table}"));
            }
        }
        clean(&mut p.a, "DSPMFSINF OBJ('/')", CMD)?;
        Ok(format!(
            "link A {} and B {}; names {A_NAME} and {B_NAME}; A's daemons {started} started, {running} running; three exports",
            links[0], links[1]
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_link_port() {
        let hw = "Resource  Type  Status\nCMN01  2838  Operational  Virtio Ethernet port eth0 52:54:00:12:34:56\nCMN02  2838  Operational  Virtio Ethernet port eth1 52:54:00:4E:46:01\n";
        assert_eq!(
            port_of(hw, A_MAC),
            Some(("CMN02".to_string(), "eth1".to_string()))
        );
        assert_eq!(port_of(hw, B_MAC), None);
    }

    #[test]
    fn link_netdevs() {
        let l = Link {
            id: "l1".into(),
            slot: 0x14,
            mac: A_MAC.into(),
            local: 4001,
            remote: 4002,
            dgram: false,
        };
        assert_eq!(
            l.netdev(),
            "socket,id=l1,udp=127.0.0.1:4002,localaddr=127.0.0.1:4001"
        );
        let d = Link { dgram: true, ..l };
        assert!(d.netdev().starts_with("dgram,id=l1,local.type=inet,"));
        assert!(d.netdev().ends_with("remote.port=4002"));
        assert!(STEPS.iter().all(|(n, _)| n.contains(" NFS ")));
    }
}
