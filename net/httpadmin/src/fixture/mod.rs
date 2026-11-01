// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's sample system, so served pages can be compared with
//! the golden pages and the demo server has something to show: system
//! `QSHQ400`, user `QSECOFR`, eight instances, their jobs, a clock
//! stopped at 10/29/26 14:11:02.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use httpd::files::{Files, MemFiles};
use httpd::instance::{self, LogFormat, Spec};
use httpd::registry::{Entry, Registry};
use qsweb::msg::{Message, SecondLevel};

use crate::env::{Audit, Certs, EndHow, Env, HttpJob, Jobs, System};
use crate::msgs;
use crate::util::{mdy, secs};

/// The fixture's password for every user.
pub const PASSWORD: &str = "QS400";

/// The fixture clock.
pub fn now() -> u64 {
    secs(2026, 10, 29, 14, 11, 2)
}

/// The sample system.
pub struct FixtureSystem {
    /// Audit entries written.
    pub audit: Mutex<Vec<Audit>>,
    /// Users: name, special authorities, class, groups.
    pub users: Vec<(&'static str, Vec<&'static str>, &'static str, Vec<&'static str>)>,
}

impl Default for FixtureSystem {
    fn default() -> Self {
        Self {
            audit: Mutex::new(Vec::new()),
            users: vec![
                (
                    "QSECOFR",
                    vec!["*ALLOBJ", "*AUDIT", "*IOSYSCFG", "*JOBCTL", "*SAVSYS", "*SECADM", "*SERVICE", "*SPLCTL"],
                    "*SECOFR",
                    vec![],
                ),
                ("QTMHHTTP", vec!["*IOSYSCFG"], "*USER", vec![]),
                ("WEBADM", vec!["*IOSYSCFG"], "*SYSOPR", vec![]),
                ("WEBUSER", vec![], "*USER", vec!["WEBUSERS"]),
            ],
        }
    }
}

impl System for FixtureSystem {
    fn name(&self) -> String {
        "QSHQ400".into()
    }

    fn now(&self) -> u64 {
        now()
    }

    fn stamp(&self, secs: u64) -> (String, String) {
        mdy(secs, 0)
    }

    fn sign_on(&self, user: &str, password: &str, _from: &str) -> Result<String, Message> {
        let u = user.trim().to_ascii_uppercase();
        let known = self.users.iter().any(|x| x.0 == u);
        if known && password == PASSWORD {
            return Ok(u);
        }
        self.audit.lock().expect("audit").push(Audit::Password {
            user: u.clone(),
            code: if known { 'P' } else { 'U' },
        });
        Err(if known {
            Message::new(qsweb::msg::Kind::Error, "CPF22E2", "Password not correct for user profile.")
        } else {
            Message::new(qsweb::msg::Kind::Error, "CPF2204", &format!("User profile {u} not found."))
        })
    }

    fn special(&self, user: &str) -> Vec<String> {
        self.users
            .iter()
            .find(|x| x.0 == user)
            .map(|x| x.1.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    fn groups(&self, user: &str) -> Vec<String> {
        self.users
            .iter()
            .find(|x| x.0 == user)
            .map(|x| x.3.iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    fn user_class(&self, user: &str) -> String {
        self.users.iter().find(|x| x.0 == user).map_or("*USER", |x| x.2).to_string()
    }

    fn profile_exists(&self, user: &str) -> bool {
        self.users.iter().any(|x| x.0.eq_ignore_ascii_case(user))
    }

    fn audit(&self, entry: Audit) {
        self.audit.lock().expect("audit").push(entry);
    }

    fn message(&self, id: &str, data: &[&str]) -> Option<SecondLevel> {
        msgs::second_level(id, data)
    }

    fn job(&self) -> String {
        "123457/QTMHHTTP/ADMIN".into()
    }

    fn interfaces(&self) -> Vec<String> {
        ["10.0.10.20", "10.0.10.21", "172.16.0.5", "127.0.0.1"].map(String::from).to_vec()
    }
}

/// The sample jobs of `QHTTPSVR`.
pub struct FixtureJobs {
    /// Active instance jobs.
    pub jobs: Mutex<Vec<HttpJob>>,
    /// The next job number.
    pub next: Mutex<u32>,
    /// Where instances live, to find a started one's ports.
    pub files: Arc<dyn Files>,
}

fn job(instance: &str, number: &str, h: u32, m: u32, s: u32, connections: u32) -> HttpJob {
    HttpJob {
        instance: instance.into(),
        number: number.into(),
        user: "QTMHHTTP".into(),
        started: secs(2026, 10, 29, h, m, s),
        connections,
    }
}

impl Jobs for FixtureJobs {
    fn active(&self) -> Vec<HttpJob> {
        self.jobs.lock().expect("jobs").clone()
    }

    fn start(&self, _user: &str, instance: &str) -> Result<Message, Message> {
        let mut jobs = self.jobs.lock().expect("jobs");
        if jobs.iter().any(|j| j.instance == instance) {
            return Err(msgs::message(qsweb::msg::Kind::Error, "HTP8103", &[instance]));
        }
        let mut n = self.next.lock().expect("jobs");
        *n += 1;
        jobs.push(HttpJob {
            instance: instance.into(),
            number: format!("{:06}", *n),
            user: "QTMHHTTP".into(),
            started: now(),
            connections: 0,
        });
        let listen = Registry::load(self.files.as_ref())
            .ok()
            .and_then(|r| r.get(instance).cloned())
            .and_then(|e| instance::load_conf(self.files.as_ref(), &e.root).ok())
            .map(|c| {
                instance::ports(&c)
                    .iter()
                    .map(|p| format!("{}:{}", p.addr.as_deref().unwrap_or("*"), p.port))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        Ok(msgs::message(qsweb::msg::Kind::Done, "HTP8001", &[instance, &listen]))
    }

    fn end(&self, user: &str, instance: &str, _how: EndHow) -> Result<Message, Message> {
        let mut jobs = self.jobs.lock().expect("jobs");
        let before = jobs.len();
        jobs.retain(|j| j.instance != instance);
        if jobs.len() == before {
            return Err(Message::new(
                qsweb::msg::Kind::Error,
                "TCP1A12",
                &format!("HTTP server instance {instance} is not active."),
            ));
        }
        Ok(msgs::message(qsweb::msg::Kind::Done, "HTP8002", &[instance, user]))
    }
}

/// The sample certificate side (filled in with the certificate pages).
pub struct FixtureCerts;

impl Certs for FixtureCerts {}

/// The sample system with handles to its parts.
pub struct Fixture {
    /// The environment the product runs over.
    pub env: Env,
    /// The system, to read its audit entries.
    pub system: Arc<FixtureSystem>,
    /// The jobs.
    pub jobs: Arc<FixtureJobs>,
    /// The IFS.
    pub files: Arc<MemFiles>,
}

/// One sample instance: name, description, ports, autostart.
struct Sample {
    name: &'static str,
    description: &'static str,
    listen: &'static [&'static str],
    autostart: bool,
}

const SAMPLES: &[Sample] = &[
    Sample {
        name: "ADMIN",
        description: "HTTP Server Administration",
        listen: &["*:2001"],
        autostart: true,
    },
    Sample {
        name: "APACHEDFT",
        description: "Default instance",
        listen: &["*:80"],
        autostart: true,
    },
    Sample {
        name: "WEBSTORE",
        description: "Web store front end",
        listen: &[],
        autostart: false,
    },
    Sample {
        name: "INTRANET",
        description: "Intranet",
        listen: &["*:8080", "*:8443"],
        autostart: true,
    },
    Sample {
        name: "API",
        description: "Order API",
        listen: &["*:9443"],
        autostart: true,
    },
    Sample {
        name: "NAVIGATOR",
        description: "Navigator for QS/400",
        listen: &["*:2002", "*:2005"],
        autostart: true,
    },
    Sample {
        name: "ACCESSWEB",
        description: "QS/400 Access for Web",
        listen: &["*:2003", "*:2006"],
        autostart: true,
    },
    Sample {
        name: "TESTSRV",
        description: "Test server",
        listen: &["*:8000"],
        autostart: false,
    },
];

fn sample_conf(s: &Sample) -> String {
    let root = instance::default_root(s.name);
    let spec = Spec {
        name: s.name.into(),
        description: s.description.into(),
        root: root.clone(),
        docroot: format!("{root}/htdocs"),
        sample: false,
        addr: None,
        port: 80,
        access_log: Some((format!("{root}/logs/access_log"), LogFormat::Combined)),
        keep_days: 30,
        user: "QTMHHTTP".into(),
        autostart: s.autostart,
        copy_from: None,
    };
    let text = instance::generate(&spec, "10/27/26 18:40:55");
    let mut lines: Vec<String> = text.lines().map(ToString::to_string).collect();
    let at = lines.iter().position(|l| l.starts_with("Listen ")).unwrap_or(3);
    lines.remove(at);
    for (i, l) in s.listen.iter().enumerate() {
        lines.insert(at + i, format!("Listen {l}"));
    }
    let tls: Vec<&&str> = s.listen.iter().skip(1).collect();
    let mut out = lines.join("\n");
    out.push('\n');
    for l in tls {
        let _ = write!(
            out,
            "\n<VirtualHost {l}>\n   SSLEngine On\n   SSLAppName QIBM_HTTP_SERVER_{}\n</VirtualHost>\n",
            s.name
        );
    }
    out
}

/// The sample system.
pub fn fixture() -> Fixture {
    let files = Arc::new(MemFiles::new());
    let mut reg = Registry {
        version: 1,
        instances: Vec::new(),
    };
    for s in SAMPLES {
        let root = instance::default_root(s.name);
        let conf = if s.name == "WEBSTORE" {
            include_str!("webstore.conf").to_string()
        } else {
            sample_conf(s)
        };
        files.put(&instance::conf_path(&root), conf.as_bytes());
        files.put(&format!("{root}/htdocs/index.html"), instance::sample_index(s.name).as_bytes());
        files.put(&format!("{root}/logs/error_log"), b"");
        let mut e = Entry {
            name: s.name.into(),
            description: s.description.into(),
            root,
            autostart: s.autostart,
            created: secs(2026, 10, 27, 18, 40, 55),
            ..Entry::default()
        };
        if s.name == "WEBSTORE" {
            e.last_started = Some(secs(2026, 10, 27, 18, 42, 10));
            e.last_ended = Some(secs(2026, 10, 28, 2, 0, 0));
            e.ended_how = "ENDTCPSVR *HTTP, controlled".into();
            e.hot_backup = Some("WEBSTORE2".into());
            files.put("/www/webstore/logs/access_log", include_bytes!("webstore-access.log"));
            // Twelve days of rotated access logs: 14 log files, 212 MB.
            for d in 17..29 {
                files.put_sized(&format!("/www/webstore/logs/access_log.2026-10-{d}"), b"", 18_525_000);
            }
        }
        reg.put(e);
    }
    reg.save(files.as_ref()).expect("fixture registry");
    let system = Arc::new(FixtureSystem::default());
    let jobs = Arc::new(FixtureJobs {
        jobs: Mutex::new(vec![
            job("ADMIN", "123456", 6, 0, 1, 1),
            job("APACHEDFT", "123461", 6, 0, 2, 3),
            job("INTRANET", "123470", 6, 0, 4, 17),
            job("API", "123471", 6, 0, 4, 5),
            job("NAVIGATOR", "123462", 6, 0, 2, 2),
            job("ACCESSWEB", "123463", 6, 0, 3, 4),
        ]),
        next: Mutex::new(123_480),
        files: files.clone(),
    });
    let env = Env {
        system: system.clone(),
        jobs: jobs.clone(),
        files: files.clone(),
        certs: Arc::new(FixtureCerts),
    };
    Fixture {
        env,
        system,
        jobs,
        files,
    }
}
