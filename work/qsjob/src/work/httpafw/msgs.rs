// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::msgs::Msgs` as the user: the profile's queue from
//! `QSYRUSRI` (`sec_api Profile`), messages from `msgq_list` (what
//! `DSPMSG` reads); changes are commands run as the user.

use accessweb::env::msgs::{Msgs, QueuedMessage};
use qsweb::msg::{Kind, Message};
use sec::api::{Answer, Query};
use stg::Client;
use wm::obj::msgq::{MsgKind, QMsg, Status};

use super::Link;

/// Message queues.
pub struct LiveMsgs {
    /// The job's thread.
    pub link: Link,
}

/// A service failure (`CPF2403 text`) as a message.
fn failure(e: &str) -> Message {
    let (id, text) = e.split_once(' ').unwrap_or(("CPF9898", e));
    Message::new(Kind::Error, id, text)
}

/// One queue entry as the pages show it.
fn queued(m: QMsg) -> QueuedMessage {
    let inquiry = m.kind == MsgKind::Inq;
    QueuedMessage {
        key: format!("{:08X}", m.key),
        id: m.msgid.trim().to_string(),
        severity: m.severity,
        inquiry,
        sent: m.time / 1_000_000,
        text: m.text,
        job: format!("{:06}/{}/{}", m.job.2, m.job.1, m.job.0),
        user: m.job.1,
        default: m.dft,
        reply: (inquiry && m.status == Status::Answered).then_some(m.reply),
    }
}

impl Msgs for LiveMsgs {
    fn own_queue(&self, user: &str) -> (String, String) {
        let u = user.trim().to_ascii_uppercase();
        let fallback = ("QUSRSYS".to_string(), u.clone());
        let q = self.link.read(user, move |m| match m.sec(Query::Profile { user: u }) {
            Ok(Answer::Profile { data }) => sec::profile::UsrPrf::decode(&data).ok().map(|p| (p.msgq.1, p.msgq.0)),
            _ => None,
        });
        match q {
            Some((lib, name)) if !name.is_empty() && !lib.starts_with('*') => (lib, name),
            Some((_, name)) if !name.is_empty() && !name.starts_with('*') => ("QUSRSYS".into(), name),
            _ => fallback,
        }
    }

    fn messages(&self, user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message> {
        let q = format!("{lib}/{name}");
        self.link
            .read(user, move |m| Some(m.ask(|l| l.msgq_list(q))))
            .unwrap_or_else(|| Err("CPF9897 Access for Web could not read the queue.".into()))
            .map(|all| all.into_iter().filter(|m| m.kind != MsgKind::Rpy).map(queued).collect())
            .map_err(|e| failure(&e))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}
