// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection job of the test [`System`].

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use qap_proto::ops::command::{CommandDefinition, CommandReply};
use qap_proto::ops::signon::{PasswordStatus, SignOnReply, TelnetTicketReply};
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{Secret, ServerMessage, Status, Token};

use super::commands::{self, definition, find, parse};
use super::{System, lock};
use crate::host::{Canceller, Ending, Host, Ran, Refusal, SystemId};
use crate::tickets::TICKET_SECS;

/// A connection job.
pub struct MemHost {
    /// The system.
    pub sys: Arc<System>,
    /// The job's number.
    pub job: u32,
}

impl MemHost {
    /// Job `job` of `sys`.
    #[must_use]
    pub fn new(sys: Arc<System>, job: u32) -> Self {
        Self { sys, job }
    }

    fn me(&self) -> Option<String> {
        lock(&self.sys.running_as).get(&self.job).cloned()
    }

    fn refuse(&mut self, status: Status, id: &str, data: &[&str]) -> Refusal {
        Refusal {
            status,
            message: self.message(id, data),
        }
    }

    fn reply(&self, token: Token, user: &str) -> SignOnReply {
        SignOnReply {
            token,
            idle_timeout_secs: self.sys.idle_secs,
            profile: user.into(),
            text: format!("{user} profile"),
            ccsid: 37,
            date_format: "*MDY".into(),
            date_separator: "/".into(),
            time_separator: ":".into(),
            home_directory: format!("/home/{user}"),
            special_authorities: Vec::new(),
        }
    }

    /// The display's checks; the profile and whether it expired.
    fn check(&mut self, user: &str, password: &str) -> Result<(String, bool), Refusal> {
        let user = user.trim().to_ascii_uppercase();
        let audit = |s: &System, e: &str| lock(&s.audit).push(format!("PW {e} {user}"));
        let mut users = lock(&self.sys.users);
        let Some(u) = users.get_mut(&user) else {
            drop(users);
            audit(&self.sys, "U");
            return Err(self.refuse(Status::SignOnFailed, "CPF1107", &[]));
        };
        if u.disabled {
            drop(users);
            audit(&self.sys, "Q");
            return Err(self.refuse(Status::SignOnFailed, "CPF1394", &[&user]));
        }
        if u.none {
            drop(users);
            return Err(self.refuse(Status::SignOnFailed, "CPF1118", &[&user]));
        }
        if u.password != password {
            u.invalid += 1;
            let n = u.invalid;
            if n >= 3 {
                u.disabled = true;
            }
            drop(users);
            audit(&self.sys, "P");
            return Err(match n {
                3.. => self.refuse(Status::SignOnFailed, "CPF1393", &[&user]),
                2 => self.refuse(Status::SignOnFailed, "CPF1392", &[]),
                _ => self.refuse(Status::SignOnFailed, "CPF1107", &[]),
            });
        }
        u.invalid = 0;
        Ok((user, u.expired))
    }

    fn open(&mut self, user: &str) -> SignOnReply {
        let token = self.sys.next_token(0x5A);
        lock(&self.sys.sessions).create(&token, user, self.sys.idle_secs, self.job);
        lock(&self.sys.running_as).insert(self.job, user.into());
        self.reply(token, user)
    }

    fn job_id(&self) -> JobId {
        JobId {
            name: "QAPCNN".into(),
            user: "QUSER".into(),
            number: format!("{:06}", self.job),
        }
    }

    fn ran(&mut self, status: Status, ids: &[(&str, &[&str])]) -> Ran {
        let messages: Vec<ServerMessage> = ids.iter().map(|(id, data)| self.message(id, data)).collect();
        let escape = (status != Status::Ok).then(|| messages.last().cloned()).flatten();
        Ran {
            status,
            escape,
            reply: CommandReply {
                job: self.job_id(),
                messages,
            },
        }
    }
}

impl Host for MemHost {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            time_zone: "QN0000UTC".into(),
            ccsid: 37,
        }
    }

    fn now_ms(&mut self) -> i64 {
        self.sys.gate.pass();
        i64::try_from(self.sys.now()).unwrap_or(0) * 1000
    }

    /// Its text says whose language it was looked up in.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        let lang = self.me().map_or_else(|| "system".to_string(), |u| format!("lang of {u}"));
        ServerMessage {
            id: id.into(),
            severity: 0,
            data: data.join(" ").into_bytes(),
            text: format!("{id} {} ({lang})", data.join(" ")),
            help: format!("Help for {id}."),
        }
    }

    fn system_info(&mut self) -> SystemInfo {
        self.sys.gate.pass();
        let n = *lock(&self.sys.products);
        SystemInfo {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            serial: "1234567".into(),
            model: "Test Machine 1".into(),
            ipl_time: 1_792_000_000_000,
            products: (0..n)
                .map(|i| Product {
                    id: format!("QS{i:05}"),
                    option: "0000".into(),
                    release: "V1R0M0".into(),
                })
                .collect(),
        }
    }

    fn ending(&mut self) -> Option<Ending> {
        lock(&self.sys.ending).take()
    }

    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal> {
        let (user, expired) = self.check(user, password.expose())?;
        if expired {
            return Err(self.refuse(Status::PasswordExpired, "CPD9BAE", &[]));
        }
        Ok(self.open(&user))
    }

    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal> {
        let me = self.me();
        let user = user.trim().to_ascii_uppercase();
        match &me {
            Some(m) if *m != user => return Err(self.refuse(Status::NotAuthorized, "CPF22E9", &[&user])),
            Some(_) => {
                if self.sys.get(&user).password != old.expose() {
                    return Err(self.refuse(Status::Error, "CPF22E2", &[&user]));
                }
            }
            None => {
                self.check(&user, old.expose())?;
            }
        }
        if new.expose().len() < 6 || new.expose() == old.expose() {
            return Err(self.refuse(Status::Error, "CPF2362", &[]));
        }
        self.sys.edit(&user, |u| {
            u.password = new.expose().into();
            u.expired = false;
        });
        Ok(match me {
            Some(_) => self.reply(Token([0; 32]), &user),
            None => self.open(&user),
        })
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let now = self.sys.now();
        let r = lock(&self.sys.sessions).resume(token, self.job, now, &|_| true);
        match r {
            Ok(s) => {
                lock(&self.sys.running_as).insert(self.job, s.user.clone());
                Ok(self.reply(token.clone(), &s.user))
            }
            Err(_) => Err(self.refuse(Status::SessionExpired, "TCP1A53", &[])),
        }
    }

    fn sign_off(&mut self, token: &Token) {
        lock(&self.sys.sessions).end(token);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn detach(&mut self, token: &Token) {
        let now = self.sys.now();
        lock(&self.sys.sessions).detach(token, self.job, now);
        lock(&self.sys.running_as).remove(&self.job);
    }

    fn password_status(&mut self) -> PasswordStatus {
        let me = self.me().unwrap_or_default();
        let u = self.sys.get(&me);
        let ms = |s: u64| i64::try_from(s).unwrap_or(i64::MAX) * 1000;
        PasswordStatus {
            expires: if u.expired { Some(ms(self.sys.now())) } else { u.expires.map(ms) },
            warning_days: self.sys.warning_days,
        }
    }

    fn telnet_ticket(&mut self) -> Result<TelnetTicketReply, Refusal> {
        let me = self.me().unwrap_or_default();
        let u = self.sys.get(&me);
        if !self.sys.auto_signon.load(Ordering::SeqCst) || u.disabled || u.none || u.expired {
            return Err(self.refuse(Status::NotAuthorized, "TCP1A55", &[&me]));
        }
        let t = self.sys.next_token(0x7E);
        lock(&self.sys.tickets).issue(&t, &me, self.sys.now());
        lock(&self.sys.audit).push(format!("PS P {me}"));
        Ok(TelnetTicketReply {
            ticket: t,
            expires_secs: TICKET_SECS,
        })
    }

    fn run_command(&mut self, command: &str) -> Ran {
        self.sys.cancel.store(false, Ordering::SeqCst);
        let (name, parms) = parse(command);
        let Some(c) = find(&name) else {
            return self.ran(Status::NotFound, &[("CPD0030", &[&name, "*LIBL"]), ("CPF0006", &[])]);
        };
        if !c.authorized {
            return self.ran(Status::NotAuthorized, &[("CPF9802", &["*CMD", c.name])]);
        }
        if c.interactive_only {
            return self.ran(Status::Error, &[("CPD0031", &[c.name]), ("CPF0006", &[])]);
        }
        let get = |k: &str| parms.iter().find(|(p, _)| p == k).map(|(_, v)| v.clone());
        if let Some(missing) = c.required.iter().find(|k| get(k).is_none()) {
            return self.ran(Status::Error, &[("CPD0071", &[missing]), ("CPF0006", &[])]);
        }
        match c.name {
            "CRTLIB" => {
                let lib = get("LIB").unwrap_or_default().to_ascii_uppercase();
                let mut libs = lock(&self.sys.libraries);
                if libs.contains(&lib) {
                    drop(libs);
                    return self.ran(Status::Error, &[("CPF2111", &[&lib])]);
                }
                libs.push(lib.clone());
                drop(libs);
                self.ran(Status::Ok, &[("CPC2102", &[&lib])])
            }
            "DSPOBJD" if get("OBJ").is_some_and(|o| o.eq_ignore_ascii_case("QSYS/SECRET")) => {
                self.ran(Status::NotAuthorized, &[("CPF9802", &["*FILE", "SECRET"])])
            }
            "DLYJOB" => {
                let secs = get("DLY").and_then(|d| d.trim().parse::<u64>().ok()).unwrap_or(1);
                let until = Instant::now() + Duration::from_secs(secs);
                while Instant::now() < until {
                    if self.sys.cancel.swap(false, Ordering::SeqCst) {
                        return self.ran(Status::Cancelled, &[("CPF1907", &[])]);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                self.ran(Status::Ok, &[])
            }
            _ => self.ran(Status::Ok, &[]),
        }
    }

    fn check_command(&mut self, command: &str) -> (Status, Vec<ServerMessage>) {
        let (name, parms) = parse(command);
        let Some(c) = commands::find(&name) else {
            return (Status::NotFound, vec![self.message("CPD0030", &[&name, "*LIBL"])]);
        };
        match c.required.iter().find(|k| !parms.iter().any(|(p, _)| p == *k)) {
            Some(missing) => (
                Status::Error,
                vec![self.message("CPD0071", &[missing]), self.message("CPF0006", &[])],
            ),
            None => (Status::Ok, Vec::new()),
        }
    }

    fn describe_command(&mut self, command: &ObjectName) -> Result<CommandDefinition, Refusal> {
        let lib = command.library.trim().to_ascii_uppercase();
        let found = find(&command.name).filter(|_| matches!(lib.as_str(), "" | "*LIBL" | "QSYS"));
        match found {
            None => Err(self.refuse(Status::NotFound, "CPD0030", &[&command.name, &lib])),
            Some(c) if !c.authorized => Err(self.refuse(Status::NotAuthorized, "CPF9802", &["*CMD", c.name])),
            Some(c) => Ok(definition(c)),
        }
    }

    fn canceller(&mut self) -> Option<Canceller> {
        let flag = Arc::clone(&self.sys.cancel);
        Some(Arc::new(move || flag.store(true, Ordering::SeqCst)))
    }
}
