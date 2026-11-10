// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::netsvr::NetServer` as the user: every read and
//! change a `QZLS*` API call through the storage service
//! (`work::nsadmin`) with the job swapped to the signed-on user, so the
//! API checks that user's authority; a refusal is the API's message
//! from `QCPFMSG`.

#![allow(clippy::doc_markdown)] // NetServer

use navigator::env::netsvr::{Attributes, Disabled, NetServer, Session, Settings, Share, Status};
use netsvrcfg::client::{self, Api, Failure};
use qsweb::msg::{Kind, Message};

use super::{Link, gone};
use crate::work::nsadmin;
use crate::work::web::bridge::Main;

/// NetServer.
pub struct LiveNetServer {
    /// The job's thread.
    pub link: Link,
}

/// The message of a refused call, from the message file.
fn message(m: &Main, f: &Failure) -> Message {
    let id = f.id.clone();
    match m.ask(|l| l.describe(id, "QCPFMSG".into(), "*LIBL".into())) {
        Some(d) => {
            let vars = d.render_vars(&f.data);
            let refs: Vec<&str> = vars.iter().map(String::as_str).collect();
            Message::new(Kind::Error, &f.id, &d.format(&f.data)).with_data(&refs)
        }
        None => Message::new(Kind::Error, &f.id, &f.id),
    }
}

impl LiveNetServer {
    /// `f` over the APIs as `user`.
    fn api<T: Send + 'static>(
        &self,
        user: &str,
        f: impl FnOnce(&mut dyn Api) -> Result<T, Failure> + Send + 'static,
    ) -> Result<T, Message> {
        let u = user.to_string();
        self.link
            .bridge
            .call(move |m| {
                m.as_user(&u, |m| {
                    let mut call = |name: &str, parms: Vec<Vec<u8>>| {
                        m.ask(|l| l.netsvr_api(name.to_string(), parms))
                            .map_err(|(id, data)| Failure { id, data })
                    };
                    let r = f(&mut call);
                    r.map_err(|e| message(m, &e))
                })
            })
            .unwrap_or_else(|| Err(gone()))
            .and_then(|r| r)
    }
}

impl NetServer for LiveNetServer {
    fn status(&self, user: &str) -> Status {
        self.link.read(user, |m| {
            let job = m.ask(|l| Ok(nsadmin::server(l)));
            Status {
                active: job.is_some(),
                job: job.unwrap_or_default(),
            }
        })
    }

    fn shares(&self, user: &str) -> Result<Vec<Share>, Message> {
        self.api(user, |a| client::list_shares(a, 37))
    }

    fn attributes(&self, user: &str) -> Result<Attributes, Message> {
        self.api(user, client::read_attributes)
    }

    fn sessions(&self, user: &str) -> Vec<Session> {
        self.link
            .read(user, |m| m.ask(|l| Ok(nsadmin::sessions(l))))
    }

    fn disabled(&self, user: &str) -> Vec<Disabled> {
        self.link.read(user, |m| {
            m.ask(|l| Ok(nsadmin::disabled(l)))
                .into_iter()
                .map(|(user, since)| Disabled { user, since })
                .collect()
        })
    }

    fn add_share(&self, user: &str, s: &Share) -> Result<(), Message> {
        let s = s.clone();
        self.api(user, move |a| client::add_share(a, &s))
    }

    fn change_share(&self, user: &str, s: &Share) -> Result<(), Message> {
        let s = s.clone();
        self.api(user, move |a| client::change_share(a, &s))
    }

    fn remove_share(&self, user: &str, name: &str) -> Result<(), Message> {
        let n = name.to_string();
        self.api(user, move |a| client::remove_share(a, &n))
    }

    fn change_attributes(&self, user: &str, from: &Settings, to: &Settings) -> Result<(), Message> {
        let (from, to) = (from.clone(), to.clone());
        self.api(user, move |a| client::change_attributes(a, &from, &to))
    }

    fn start(&self, user: &str, reset: bool) -> Result<(), Message> {
        self.api(user, move |a| client::start(a, reset))
    }

    fn end(&self, user: &str) -> Result<(), Message> {
        self.api(user, client::end)
    }

    fn end_session(&self, user: &str, id: u64) -> Result<(), Message> {
        self.api(user, move |a| client::end_session(a, id))
    }

    fn enable(&self, user: &str, name: &str) -> Result<(), Message> {
        let n = name.to_string();
        self.api(user, move |a| client::enable_users(a, &[n]))
    }
}
