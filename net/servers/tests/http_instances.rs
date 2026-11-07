// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*HTTP` with registry instances against a fake job: start, already
//! started, unknown, end with a delay, `*ALL`, `STRTCP`'s autostart,
//! the administration instance beside them, and Navigator's, there
//! only while `QS00NAV` is installed.

use servers::framework::{HttpInstance, HttpServer};
use servers::{Env, Framework, Instance, JobStart, Request};
use tcpcfg::model::svr::{ServerAttrs, Servers};

#[derive(Default)]
struct Fake {
    cfg: Servers,
    instances: Vec<HttpInstance>,
    running: Vec<JobStart>,
    subsystems: Vec<String>,
    endings: Vec<(String, bool, u32)>,
    /// `QS00NAV` deleted.
    no_navigator: bool,
}

impl Env for Fake {
    fn active(&mut self, job: &str) -> bool {
        self.running.iter().any(|j| j.name == job)
    }
    fn start_job(&mut self, job: &JobStart) -> Result<(), String> {
        self.running.push(job.clone());
        Ok(())
    }
    fn end_job(&mut self, job: &str, _immediate: bool) -> Result<(), String> {
        self.running.retain(|j| j.name != job);
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
    fn http_instances(&mut self) -> Vec<HttpInstance> {
        self.instances.clone()
    }
    fn http_ending(&mut self, instance: &str, immediate: bool, delay: u32) {
        self.endings.push((instance.into(), immediate, delay));
    }
    fn product_installed(&mut self, product: &str) -> bool {
        !(self.no_navigator && product == "QS00NAV")
    }
}

fn fw() -> Framework {
    let mut f = Framework::standard();
    f.register(Box::new(HttpServer));
    f
}

fn env() -> Fake {
    Fake {
        instances: vec![
            HttpInstance {
                name: "WEB1".into(),
                autostart: true,
                user: "QTMHHTTP".into(),
            },
            HttpInstance {
                name: "SHOP".into(),
                autostart: false,
                user: "WEBUSER".into(),
            },
        ],
        ..Fake::default()
    }
}

fn named(n: &str) -> Request {
    Request {
        instance: Instance::Named(n.into()),
        ..Request::new("*HTTP")
    }
}

#[test]
fn one_instance_starts_and_ends() {
    let (fw, mut env) = (fw(), env());
    assert_eq!(
        fw.start(&mut env, &named("shop")).unwrap(),
        "TCP1A07 HTTP server starting: instance SHOP."
    );
    let j = &env.running[0];
    assert_eq!(
        (
            j.name.as_str(),
            j.user.as_str(),
            j.subsystem.as_str(),
            j.role.as_str(),
            j.rqsdta.as_str()
        ),
        ("SHOP", "WEBUSER", "QHTTPSVR", "HTTPSVR", "INSTANCE(SHOP)")
    );
    assert_eq!(env.subsystems, ["QHTTPSVR"]);
    let again = fw.start(&mut env, &named("SHOP")).unwrap_err();
    assert_eq!(
        (again.msgid.as_str(), again.text.as_str()),
        ("TCP1A08", "HTTP server instance SHOP already started.")
    );
    assert_eq!(
        fw.start(&mut env, &named("NOPE")).unwrap_err().msgid,
        "TCP1A06"
    );
    assert_eq!(
        fw.end(&mut env, &named("NOPE"), false).unwrap_err().msgid,
        "TCP1A06"
    );
    let mut end = named("SHOP");
    end.delay = Some(90);
    assert_eq!(
        fw.end(&mut env, &end, false).unwrap(),
        "TCP1A0A HTTP server ending: instance SHOP."
    );
    assert_eq!(env.endings, [("SHOP".to_string(), false, 90)]);
    assert_eq!(
        fw.end(&mut env, &named("SHOP"), true).unwrap_err().msgid,
        "TCP1A09"
    );
    // The administration instance is still *ADMIN (or ADMIN).
    assert_eq!(
        fw.start(&mut env, &named("ADMIN")).unwrap(),
        "TCP1A07 HTTP server starting."
    );
    assert_eq!(env.running[0].role, "HTTPADM");
}

#[test]
fn all_and_autostart() {
    let (fw, mut env) = (fw(), env());
    let out = fw.run(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        true,
        false,
    );
    let text = out[0].1.as_ref().unwrap();
    assert_eq!(
        text,
        "TCP1A07 HTTP server starting.\nTCP1A07 HTTP server starting: instance WEB1."
    );
    let names: Vec<&str> = env.running.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["ADMIN", "WEB1"]);
    // Ending *ALL ends both, with the delay given.
    let out = fw.run_with(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        false,
        false,
        Some(5),
    );
    assert_eq!(out[0].1.as_ref().unwrap().lines().count(), 2);
    assert!(env.running.is_empty());
    assert_eq!(env.endings, [("WEB1".to_string(), false, 5)]);
    assert_eq!(
        fw.end(&mut env, &Request::new("*HTTP"), false).unwrap(),
        "TCP1A09 HTTP server not active."
    );
    // STRTCP: *HTTP's own autostart is off, so only WEB1.
    assert_eq!(
        fw.start(&mut env, &Request::autostart("*HTTP")).unwrap(),
        "TCP1A07 HTTP server starting: instance WEB1."
    );
    assert!(fw.active(&mut env, "*HTTP"));
    assert_eq!(
        fw.start(&mut env, &Request::autostart("*HTTP")).unwrap(),
        ""
    );
}

/// `CHGHTTPA AUTOSTART(*YES)` turns the table's flag on: `STRTCP` then
/// starts the administration instance too, and `*AUTOSTART` names `*HTTP`.
#[test]
fn chghttpa_autostart() {
    let (fw, mut env) = (fw(), env());
    assert_eq!(
        servers::registry::lookup("*HTTP").unwrap().attrs_cmd,
        Some("CHGHTTPA")
    );
    assert!(
        !fw.expand(&mut env, &["*AUTOSTART".to_string()], true)
            .contains(&"*HTTP".to_string())
    );
    env.cfg.get_mut("*HTTP").autostart = true;
    assert!(
        fw.expand(&mut env, &["*AUTOSTART".to_string()], true)
            .contains(&"*HTTP".to_string())
    );
    assert_eq!(
        fw.start(&mut env, &Request::autostart("*HTTP")).unwrap(),
        "TCP1A07 HTTP server starting.\nTCP1A07 HTTP server starting: instance WEB1."
    );
    let names: Vec<&str> = env.running.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["ADMIN", "WEB1"]);
}

/// `HTTPSVR(NAVIGATOR)` starts and ends Navigator's job, its ports from
/// `CHGHTTPA`; its registry entry (made by `ADMIN`) is not a user
/// instance, and it starts with `*ALL` only when marked autostart.
#[test]
fn navigator_is_an_instance_of_its_own() {
    let (fw, mut env) = (fw(), env());
    env.cfg.get_mut("*HTTP").set("NAVPORT", "8002");
    env.instances.push(HttpInstance {
        name: "NAVIGATOR".into(),
        autostart: false,
        user: "QTMHHTTP".into(),
    });
    assert_eq!(
        fw.start(&mut env, &named("navigator")).unwrap(),
        "TCP1A07 HTTP server starting: instance NAVIGATOR."
    );
    let j = &env.running[0];
    assert_eq!(
        (
            j.name.as_str(),
            j.user.as_str(),
            j.subsystem.as_str(),
            j.role.as_str(),
            j.rqsdta.as_str()
        ),
        (
            "NAVIGATOR",
            "QTMHHTTP",
            "QHTTPSVR",
            "HTTPNAV",
            "PORT(8002) TLSPORT(2005)"
        )
    );
    assert_eq!(
        fw.start(&mut env, &named("NAVIGATOR")).unwrap_err().msgid,
        "TCP1A08"
    );
    // Ending the administration instance leaves Navigator running.
    fw.start(&mut env, &named("*ADMIN")).unwrap();
    fw.end(&mut env, &named("*ADMIN"), false).unwrap();
    let names: Vec<&str> = env.running.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["NAVIGATOR"]);
    assert_eq!(
        fw.end(&mut env, &named("NAVIGATOR"), true).unwrap(),
        "TCP1A0A HTTP server ending: instance NAVIGATOR."
    );
    assert_eq!(
        fw.end(&mut env, &named("NAVIGATOR"), true)
            .unwrap_err()
            .msgid,
        "TCP1A09"
    );
    // Marked autostart, *ALL starts it with the rest.
    env.instances.last_mut().unwrap().autostart = true;
    fw.run(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        true,
        false,
    );
    let names: Vec<&str> = env.running.iter().map(|j| j.name.as_str()).collect();
    assert_eq!(names, ["ADMIN", "NAVIGATOR", "WEB1"]);
    fw.run_with(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        false,
        false,
        None,
    );
    assert!(env.running.is_empty());
    // Without QS00NAV there is no such instance, whatever the registry says.
    env.no_navigator = true;
    let e = fw.start(&mut env, &named("NAVIGATOR")).unwrap_err();
    assert_eq!(e.msgid, "TCP1A06");
    assert!(e.text.contains("QS00NAV"), "{}", e.text);
    fw.run(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        true,
        false,
    );
    assert!(!env.running.iter().any(|j| j.name == "NAVIGATOR"));
}

/// `HTTPSVR(ACCESSWEB)` is Access for Web's job, as Navigator's: its
/// ports from `CHGHTTPA`'s `AFWPORT` and `AFWTLSPORT`.
#[test]
fn accessweb_is_an_instance_of_its_own() {
    let (fw, mut env) = (fw(), env());
    env.cfg.get_mut("*HTTP").set("AFWPORT", "8003");
    env.instances.push(HttpInstance {
        name: "ACCESSWEB".into(),
        autostart: false,
        user: "QTMHHTTP".into(),
    });
    assert_eq!(
        fw.start(&mut env, &named("accessweb")).unwrap(),
        "TCP1A07 HTTP server starting: instance ACCESSWEB."
    );
    let j = &env.running[0];
    assert_eq!(
        (
            j.name.as_str(),
            j.user.as_str(),
            j.subsystem.as_str(),
            j.role.as_str(),
            j.rqsdta.as_str()
        ),
        (
            "ACCESSWEB",
            "QTMHHTTP",
            "QHTTPSVR",
            "HTTPAFW",
            "PORT(8003) TLSPORT(2006)"
        )
    );
    // Not a user instance: *ALL leaves it alone unless marked autostart.
    fw.run_with(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        false,
        false,
        None,
    );
    assert!(env.running.is_empty());
    fw.run(
        &mut env,
        &["*HTTP".to_string()],
        &Instance::All,
        true,
        false,
    );
    assert!(!env.running.iter().any(|j| j.name == "ACCESSWEB"));
    assert_eq!(
        fw.end(&mut env, &named("ACCESSWEB"), true)
            .unwrap_err()
            .msgid,
        "TCP1A09"
    );
}
