// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The framework against a fake job: `STRTCPSVR`, `ENDTCPSVR`, the
//! `*ALL` and `*AUTOSTART` lists, instances, servers this release does
//! not run, a server registered later, and `STRTCP`/`ENDTCP` through
//! `tcpcfg`'s starter.

use servers::framework::Standard;
use servers::{Env, Framework, Instance, JobStart, Request, TcpServer};
use tcpcfg::model::svr::{ServerAttrs, Servers};
use tcpcfg::server::{end_all, start_all};

#[derive(Default)]
struct Fake {
    cfg: Servers,
    running: Vec<JobStart>,
    subsystems: Vec<String>,
    ended: Vec<String>,
}

impl Env for Fake {
    fn active(&mut self, job: &str) -> bool {
        self.running.iter().any(|j| j.name == job)
    }
    fn start_job(&mut self, job: &JobStart) -> Result<(), String> {
        if job.name == "QTOTNTP" && self.cfg.get("*NTP").is_some_and(|a| a.get("FAIL").is_some()) {
            return Err("no process".into());
        }
        self.running.push(job.clone());
        Ok(())
    }
    fn end_job(&mut self, job: &str, _immediate: bool) -> Result<(), String> {
        self.running.retain(|j| j.name != job);
        self.ended.push(job.to_string());
        Ok(())
    }
    fn attrs(&mut self, server: &str) -> ServerAttrs {
        self.cfg.get(server).cloned().unwrap_or(ServerAttrs {
            server: server.into(),
            autostart: false,
            bag: Vec::new(),
        })
    }
    fn start_subsystem(&mut self, name: &str) -> Result<(), String> {
        if !self.subsystems.iter().any(|s| s == name) {
            self.subsystems.push(name.into());
        }
        Ok(())
    }
}

fn list(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| (*x).to_string()).collect()
}

#[test]
fn telnet_starts_once_and_ends() {
    let fw = Framework::standard();
    let mut env = Fake::default();
    let r = fw.start(&mut env, &Request::new("*telnet")).unwrap();
    assert_eq!(r, "TCP1A07 TELNET server starting.");
    let j = &env.running[0];
    assert_eq!((j.name.as_str(), j.user.as_str(), j.subsystem.as_str(), j.role.as_str()), ("QTVTELNET", "QTCP", "QSYSWRK", "TELNET"));
    assert_eq!(fw.start(&mut env, &Request::new("*TELNET")).unwrap(), "TCP1A08 TELNET server already started.");
    assert!(fw.active(&mut env, "*TELNET"));
    assert_eq!(fw.end(&mut env, &Request::new("*TELNET"), false).unwrap(), "TCP1A0A TELNET server ending.");
    assert_eq!(env.ended, ["QTVTELNET"]);
    assert_eq!(fw.end(&mut env, &Request::new("*TELNET"), false).unwrap(), "TCP1A09 TELNET server not active.");
}

#[test]
fn http_admin_instance_and_subsystem() {
    let fw = Framework::standard();
    let mut env = Fake::default();
    env.cfg.get_mut("*HTTP").set("PORT", "8001");
    let req = Request {
        instance: Instance::Named("*admin".into()),
        ..Request::new("*HTTP")
    };
    assert_eq!(fw.start(&mut env, &req).unwrap(), "TCP1A07 HTTP server starting.");
    assert_eq!(env.subsystems, ["QHTTPSVR"]);
    let j = &env.running[0];
    assert_eq!((j.name.as_str(), j.user.as_str(), j.role.as_str()), ("ADMIN", "QTMHHTTP", "HTTPADM"));
    assert_eq!(j.rqsdta, "PORT(8001) TLSPORT(2005)");
    let other = Request {
        instance: Instance::Named("WEB1".into()),
        ..Request::new("*HTTP")
    };
    let e = fw.start(&mut env, &other).unwrap_err();
    assert_eq!(e.msgid, "TCP1A06");
    assert!(fw.end(&mut env, &other, false).is_err());
    assert_eq!(fw.end(&mut env, &req, false).unwrap(), "TCP1A0A HTTP server ending.");
}

#[test]
fn servers_not_in_this_release() {
    let fw = Framework::standard();
    let mut env = Fake::default();
    let e = fw.start(&mut env, &Request::new("*SNMP")).unwrap_err();
    assert_eq!(e.msgid, "TCP1A04");
    assert!(e.text.contains("*SNMP"), "{}", e.text);
    let e = fw.start(&mut env, &Request::new("*DDM")).unwrap_err();
    assert!(e.text.contains("backends"), "{}", e.text);
    assert_eq!(fw.end(&mut env, &Request::new("*SMTP"), false).unwrap(), "TCP1A09 SMTP server not active.");
    assert_eq!(fw.start(&mut env, &Request::new("*NOSUCH")).unwrap_err().msgid, "TCP1A04");
}

#[test]
fn a_server_registered_later() {
    let mut fw = Framework::standard();
    fw.register(Box::new(Standard::of("*NTP").unwrap()));
    let mut env = Fake::default();
    assert_eq!(fw.start(&mut env, &Request::new("*NTP")).unwrap(), "TCP1A07 NTP server starting.");
    assert_eq!(env.running[0].user, "QNTP");
    assert_eq!(fw.registered(), ["*TELNET", "*HTTP", "*NTP"]);
    // A job that cannot start.
    let mut env = Fake::default();
    env.cfg.get_mut("*NTP").set("FAIL", "1");
    let e = fw.start(&mut env, &Request::new("*NTP")).unwrap_err();
    assert_eq!(e.msgid, "TCP1A05");
    assert!(Standard::of("*NOPE").is_none());
    let t: &dyn TcpServer = fw.get("*ntp").unwrap();
    assert_eq!(t.spec().jobs[0].name, "QTOTNTP");
}

#[test]
fn all_and_autostart_lists() {
    let fw = Framework::standard();
    let mut env = Fake::default();
    // *AUTOSTART: TELNET is on by default, HTTP off.
    let out = fw.run(&mut env, &list(&["*AUTOSTART"]), &Instance::Default, true, false);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "*TELNET");
    env.cfg.get_mut("*HTTP").autostart = true;
    let out = fw.run(&mut env, &list(&["*AUTOSTART"]), &Instance::Default, true, false);
    assert_eq!(out.len(), 2);
    assert!(out[0].1.as_ref().unwrap().contains("already started"));
    // *ALL ends only what runs.
    let out = fw.run(&mut env, &list(&["*ALL"]), &Instance::Default, false, false);
    assert_eq!(out.len(), 2);
    assert!(env.running.is_empty());
    let out = fw.run(&mut env, &list(&["*ALL", "*TELNET"]), &Instance::Default, true, false);
    assert_eq!(out.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["*TELNET", "*HTTP"]);
}

#[test]
fn strtcp_and_endtcp_through_the_starter() {
    let fw = Framework::standard();
    let mut env = Fake::default();
    let autostart = env.cfg.autostart();
    assert_eq!(autostart, ["*TELNET", "*FTP"]);
    {
        let mut s = servers::framework::Starter::new(&fw, &mut env);
        let r = start_all(&mut s, &autostart);
        assert_eq!(r[0].1, Ok("TCP1A07 TELNET server starting.".to_string()));
        assert!(r[1].1.as_ref().unwrap_err().starts_with("TCP1A04 *FTP"));
    }
    // CHGTELNA AUTOSTART(*NO) keeps it down at the next STRTCP.
    env.running.clear();
    env.cfg.get_mut("*TELNET").autostart = false;
    {
        let mut s = servers::framework::Starter::new(&fw, &mut env);
        let r = start_all(&mut s, &list(&["*TELNET"]));
        assert_eq!(r[0].1, Ok(String::new()));
    }
    assert!(env.running.is_empty());
    env.cfg.get_mut("*TELNET").autostart = true;
    let mut s = servers::framework::Starter::new(&fw, &mut env);
    start_all(&mut s, &list(&["*TELNET"]));
    let every: Vec<String> = tcpcfg::model::svr::STANDARD.iter().map(|(s, _)| (*s).to_string()).collect();
    let e = end_all(&mut s, &every, true);
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].1, Ok("TCP1A0A TELNET server ending.".to_string()));
}

#[test]
fn ftp_starts_its_listener() {
    let mut fw = Framework::standard();
    fw.register(Box::new(servers::framework::Ftp));
    let mut env = Fake::default();
    env.cfg.get_mut("*FTP").set("PORT", "2121");
    let req = Request::new("*FTP");
    assert_eq!(fw.start(&mut env, &req).unwrap(), "TCP1A07 FTP server starting.");
    let j = &env.running[0];
    assert_eq!((j.name.as_str(), j.user.as_str(), j.role.as_str()), ("QTFTP00001", "QTCP", "FTP"));
    assert_eq!((j.subsystem.as_str(), j.rqsdta.as_str()), ("QSYSWRK", "PORT(2121) TLSPORT(990)"));
    assert!(fw.start(&mut env, &req).unwrap().contains("already started"));
    assert_eq!(fw.end(&mut env, &req, false).unwrap(), "TCP1A0A FTP server ending.");
    assert!(env.running.is_empty());
    assert_eq!(fw.end(&mut env, &req, false).unwrap(), "TCP1A09 FTP server not active.");
}

#[test]
fn host_servers_start_every_daemon() {
    let mut fw = Framework::standard();
    fw.register(Box::new(servers::framework::HostSvr));
    let mut env = Fake::default();
    let r = fw.start(&mut env, &Request::new("*HOSTSVR")).unwrap();
    assert_eq!(r, "TCP1A07 HOSTSVR server starting.");
    assert_eq!(env.running.len(), 8);
    let db = env.running.iter().find(|j| j.name == "QZDASRVSD").unwrap();
    assert_eq!(
        (db.user.as_str(), db.subsystem.as_str(), db.role.as_str(), db.rqsdta.as_str()),
        ("QUSER", "QSERVER", "HOSTSVRD", "*DATABASE")
    );
    assert_eq!((db.jobd.as_str(), db.class.as_str()), ("QSYS/QZBSJOBD", "QGPL/QCASERVR"));
    assert_eq!(fw.start(&mut env, &Request::new("*HOSTSVR")).unwrap(), "TCP1A08 HOSTSVR server already started.");
    // One daemon ended by hand comes back alone.
    env.running.retain(|j| j.name != "QZSOSGND");
    fw.start(&mut env, &Request::new("*HOSTSVR")).unwrap();
    assert_eq!(env.running.len(), 8);
    assert_eq!(env.running.last().unwrap().rqsdta, "*SIGNON");
    assert_eq!(fw.end(&mut env, &Request::new("*HOSTSVR"), true).unwrap(), "TCP1A0A HOSTSVR server ending.");
    assert!(env.running.is_empty());
    assert_eq!(env.ended.len(), 8);
    assert_eq!(fw.end(&mut env, &Request::new("*HOSTSVR"), true).unwrap(), "TCP1A09 HOSTSVR server not active.");
    // Autostart off by default: STRTCP leaves them down.
    assert_eq!(fw.start(&mut env, &Request::autostart("*HOSTSVR")).unwrap(), "");
}
