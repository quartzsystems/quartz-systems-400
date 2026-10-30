// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRTCP`, `ENDTCP`, `STRTCPIFC`, `ENDTCPIFC`, routes, attributes and
//! port rules against the fake kernel, with line states scripted.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use tcpcfg::addr::IfcAddr;
use tcpcfg::apply::{self, Ctx, StartOpts};
use tcpcfg::files::Table;
use tcpcfg::line::LineState;
use tcpcfg::model::ifc::Interface;
use tcpcfg::model::port::PortRestriction;
use tcpcfg::model::rte::{Dest, Route};
use tcpcfg::model::{LineRef, Proto};
use tcpcfg::msg;
use tcpcfg::net::fake::FakeNet;
use tcpcfg::state::{IfcStatus, RteStatus, TcpStatus};
use tcpcfg::{Config, State};

/// Lines by name: kernel link and whether varied on.
#[derive(Default)]
struct Lines {
    map: BTreeMap<String, (String, bool)>,
    varied: Vec<String>,
}

impl Lines {
    fn with(mut self, lind: &str, kernel: &str, on: bool) -> Self {
        self.map.insert(lind.into(), (kernel.into(), on));
        self
    }

    fn vary_off(&mut self, lind: &str) {
        if let Some(l) = self.map.get_mut(lind) {
            l.1 = false;
        }
    }
}

impl LineState for Lines {
    fn exists(&self, lind: &str) -> bool {
        self.map.contains_key(lind)
    }
    fn interface(&self, lind: &str) -> Option<String> {
        self.map.get(lind).map(|l| l.0.clone())
    }
    fn varied_on(&self, lind: &str) -> bool {
        self.map.get(lind).is_some_and(|l| l.1)
    }
    fn vary_on(&mut self, lind: &str) -> Result<(), String> {
        let l = self.map.get_mut(lind).ok_or("no line")?;
        l.1 = true;
        self.varied.push(lind.into());
        Ok(())
    }
}

fn eth(addr: &str, line: &str) -> Interface {
    let mut i = Interface::new(IfcAddr::parse(addr).unwrap(), LineRef::Line(line.into()));
    i.mask = Some(Ipv4Addr::new(255, 255, 255, 0));
    i
}

struct World {
    cfg: Config,
    st: State,
    net: FakeNet,
    lines: Lines,
}

impl World {
    fn new() -> Self {
        let mut cfg = Config::default();
        cfg.ifcs.add(eth("10.0.2.16", "ETHLINE")).unwrap();
        let mut off = eth("10.0.3.16", "ETHLINE");
        off.autostart = false;
        cfg.ifcs.add(off).unwrap();
        cfg.rtes
            .add(Route::new(Dest::DftRoute, 0, "10.0.2.2".parse().unwrap()))
            .unwrap();
        Self {
            cfg,
            st: State::default(),
            net: FakeNet::default().with_link("eth0", true),
            lines: Lines::default().with("ETHLINE", "eth0", false),
        }
    }

    fn ctx(&mut self) -> Ctx<'_> {
        Ctx {
            cfg: &self.cfg,
            st: &mut self.st,
            net: &mut self.net,
            lines: &mut self.lines,
            now: 1000,
        }
    }
}

#[test]
fn strtcp_starts_loopback_autostart_interfaces_and_routes() {
    let mut w = World::new();
    w.cfg.attrs.set("TCPKEEPALV", "10").unwrap();
    w.cfg.domain.host_name = "QS400".into();
    w.cfg
        .ports
        .add(PortRestriction {
            lower: 8000,
            upper: Some(8100),
            proto: Proto::Tcp,
            user: "WEB".into(),
        })
        .unwrap();
    let notes = apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap();
    assert_eq!(notes.last().unwrap().msgid, msg::STARTED);
    assert!(notes.iter().all(|n| !n.error), "{notes:?}");
    assert_eq!(w.st.tcp, TcpStatus::Active);
    assert_eq!(w.lines.varied, vec!["ETHLINE".to_string()], "the line was varied on");
    assert_eq!(w.st.ifc_status("127.0.0.1"), IfcStatus::Active);
    assert_eq!(w.st.ifc_status("10.0.2.16"), IfcStatus::Active);
    assert_eq!(w.st.ifc_status("10.0.3.16"), IfcStatus::Inactive, "AUTOSTART(*NO)");
    assert_eq!(w.net.addrs_on("eth0"), vec!["10.0.2.16/24".to_string()]);
    assert!(w.net.links[1].up);
    let dft = w.cfg.rtes.list[0].key();
    assert_eq!(w.st.rte_status(&dft), RteStatus::Active);
    assert_eq!(w.net.routes.len(), 1);
    assert_eq!(w.net.sysctls["ipv4/tcp_keepalive_time"], "600");
    assert_eq!(w.net.hostname, "qs400");
    assert_eq!(w.net.rules, vec![(Proto::Tcp, 8000, 8100)]);
    assert_eq!(
        apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap_err().msgid,
        msg::ALREADY_ACTIVE
    );
    let snap = w.st.snapshot(&w.cfg);
    assert_eq!(snap.tcp, "Active");
    assert_eq!(snap.ifcs.iter().find(|i| i.addr == "10.0.2.16").unwrap().link, "eth0");
}

#[test]
fn strifc_no_starts_only_the_loopback() {
    let mut w = World::new();
    apply::start_tcp(
        &mut w.ctx(),
        StartOpts {
            strifc: false,
            strip6: true,
        },
    )
    .unwrap();
    assert_eq!(w.st.ifc_status("127.0.0.1"), IfcStatus::Active);
    assert_eq!(w.st.ifc_status("10.0.2.16"), IfcStatus::Inactive);
    assert!(w.net.routes.is_empty(), "no interface reaches the next hop");
    let dft = w.cfg.rtes.list[0].key();
    assert_eq!(w.st.rte_status(&dft), RteStatus::Inactive);
    apply::start_ifc(&mut w.ctx(), "10.0.2.16").unwrap();
    assert_eq!(w.st.rte_status(&dft), RteStatus::Active, "the route follows its interface");
    apply::end_ifc(&mut w.ctx(), "10.0.2.16").unwrap();
    assert_eq!(w.st.rte_status(&dft), RteStatus::Inactive);
    assert!(w.net.routes.is_empty());
    assert!(w.net.addrs_on("eth0").is_empty());
    assert_eq!(apply::end_ifc(&mut w.ctx(), "10.0.2.16").unwrap_err().msgid, msg::IFC_INACTIVE);
}

#[test]
fn endtcp_takes_everything_out() {
    let mut w = World::new();
    w.cfg
        .ports
        .add(PortRestriction {
            lower: 23,
            upper: None,
            proto: Proto::Tcp,
            user: "QTCP".into(),
        })
        .unwrap();
    apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap();
    let notes = apply::end_tcp(&mut w.ctx()).unwrap();
    assert_eq!(notes.last().unwrap().msgid, msg::ENDED);
    assert_eq!(w.st.tcp, TcpStatus::Inactive);
    assert!(w.net.addrs_on("eth0").is_empty());
    assert!(w.net.routes.is_empty());
    assert!(w.net.rules.is_empty());
    assert_eq!(
        w.net.addrs_on("lo"),
        vec!["127.0.0.1/8".to_string(), "::1/128".to_string()],
        "the loopback addresses stay for the system's own use"
    );
    assert_eq!(w.st.ifc_status("127.0.0.1"), IfcStatus::Inactive);
    assert_eq!(apply::end_tcp(&mut w.ctx()).unwrap_err().msgid, msg::NOT_ACTIVE);
    assert_eq!(
        apply::start_ifc(&mut w.ctx(), "10.0.2.16").unwrap_err().msgid,
        msg::NOT_AVAILABLE
    );
}

#[test]
fn interface_failures() {
    let mut w = World::new();
    w.cfg.ifcs.add(eth("10.0.9.1", "NOLINE")).unwrap();
    let mut v = eth("10.0.8.1", "ETHLINE");
    v.vlan = Some(12);
    w.cfg.ifcs.add(v).unwrap();
    w.net.failing.insert("set_mtu".into());
    let mut big = eth("10.0.7.1", "ETHLINE");
    big.mtu = Some(9000);
    w.cfg.ifcs.add(big).unwrap();
    let notes = apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap();
    let ids: Vec<&str> = notes.iter().filter(|n| n.error).map(|n| n.msgid.as_str()).collect();
    assert_eq!(ids, vec![msg::LINE_NOT_FOUND, msg::IFC_FAILED], "{notes:?}");
    assert_eq!(w.st.ifc_status("10.0.9.1"), IfcStatus::Failed);
    assert_eq!(w.st.ifc_status("10.0.7.1"), IfcStatus::Failed);
    assert_eq!(w.st.ifc_status("10.0.8.1"), IfcStatus::Active);
    assert_eq!(w.net.addrs_on("eth0.12"), vec!["10.0.8.1/24".to_string()]);
    apply::end_ifc(&mut w.ctx(), "10.0.8.1").unwrap();
    assert!(w.net.links.iter().all(|l| l.name != "eth0.12"), "the VLAN link it made is gone");
    // A failed interface can be started again once fixed.
    w.net.failing.clear();
    apply::start_ifc(&mut w.ctx(), "10.0.7.1").unwrap();
    assert_eq!(w.net.links.iter().find(|l| l.name == "eth0").unwrap().mtu, 9000);
}

#[test]
fn carrier_and_vary_off_change_interface_status() {
    let mut w = World::new();
    apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap();
    w.net.set_carrier("eth0", false);
    let n = apply::refresh(&mut w.ctx());
    assert_eq!(w.st.ifc_status("10.0.2.16"), IfcStatus::RcyPnd, "{n:?}");
    let dft = w.cfg.rtes.list[0].key();
    assert_eq!(w.st.rte_status(&dft), RteStatus::Inactive);
    w.net.set_carrier("eth0", true);
    apply::refresh(&mut w.ctx());
    assert_eq!(w.st.ifc_status("10.0.2.16"), IfcStatus::Active);
    assert_eq!(w.st.rte_status(&dft), RteStatus::Active);
    w.lines.vary_off("ETHLINE");
    apply::refresh(&mut w.ctx());
    assert_eq!(w.st.ifc_status("10.0.2.16"), IfcStatus::Inactive);
    assert!(w.net.addrs_on("eth0").is_empty());
}

#[test]
fn virtual_ip_and_changes_while_active() {
    let mut w = World::new();
    let mut vip = Interface::new(IfcAddr::parse("10.0.2.20").unwrap(), LineRef::VirtualIp);
    vip.mask = Some(Ipv4Addr::BROADCAST);
    w.cfg.ifcs.add(vip).unwrap();
    apply::start_tcp(&mut w.ctx(), StartOpts::default()).unwrap();
    assert_eq!(w.net.addrs_on(apply::VIP_LINK), vec!["10.0.2.20/32".to_string()]);
    // A route added while active goes in at once.
    w.cfg
        .rtes
        .add(Route::new(Dest::parse("192.168.50.0").unwrap(), 24, "10.0.2.3".parse().unwrap()))
        .unwrap();
    apply::on_change(&mut w.ctx(), Table::Rte);
    assert_eq!(w.net.routes.len(), 2);
    let gone = w.cfg.rtes.list.remove(1).key();
    apply::on_change(&mut w.ctx(), Table::Rte);
    assert_eq!(w.net.routes.len(), 1);
    assert!(!w.st.rtes.contains_key(&gone));
    // A route the kernel refuses is Failed with a diagnostic.
    w.net.failing.insert("add_route".into());
    w.cfg
        .rtes
        .add(Route::new(Dest::parse("192.168.60.0").unwrap(), 24, "10.0.2.3".parse().unwrap()))
        .unwrap();
    let n = apply::on_change(&mut w.ctx(), Table::Rte);
    assert_eq!(n[0].msgid, msg::RTE_FAILED);
    let k = w.cfg.rtes.list[1].key();
    assert_eq!(w.st.rte_status(&k), RteStatus::Failed);
    w.cfg.attrs.set("IPTTL", "32").unwrap();
    apply::on_change(&mut w.ctx(), Table::TcpIp);
    assert_eq!(w.net.sysctls["ipv4/ip_default_ttl"], "32");
    let report = apply::report(&w.cfg, &w.st, &mut w.net, "");
    assert!(report.iter().any(|l| l.starts_with("ifc 10.0.2.20 /32 *VIRTUALIP Active")), "{report:#?}");
    assert!(report.iter().any(|l| l.contains("kernel route default/0 via 10.0.2.2 dev eth0 qs400")), "{report:#?}");
}
