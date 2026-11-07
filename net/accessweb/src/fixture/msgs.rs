// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's messages: the three of `web/messages.html` on
//! `QUSRSYS/TESTUSER`, one an inquiry waiting for its reply; `SNDMSG`,
//! `SNDRPYMSG` and `RMVMSG` change them as the system would.

use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use crate::env::msgs::{Msgs, QueuedMessage};
use crate::util::secs;

/// A sample message: key, sent (UTC month, day, hour, minute, second of 2026), severity, ID, text, job, user,
/// inquiry, default reply.
pub type Sample = (
    &'static str,
    [u32; 5],
    u8,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    &'static str,
);

/// `TESTUSER`'s messages, oldest first.
pub const MESSAGES: &[Sample] = &[
    (
        "00000001",
        [10, 28, 21, 41, 30],
        80,
        "",
        "PRT02 is out of labels, holding your INVLABEL file until tomorrow.",
        "123301/OPERATOR/QPADEV0001",
        "OPERATOR",
        false,
        "",
    ),
    (
        "00000002",
        [10, 29, 17, 5, 10],
        0,
        "CPF1241",
        "Job 123488/TESTUSER/QPADEV0003 submitted to job queue QBATCH in QGPL.",
        "123488/TESTUSER/QPADEV0003",
        "TESTUSER",
        false,
        "",
    ),
    (
        "00000003",
        [10, 29, 17, 58, 5],
        99,
        "CPA0702",
        "CPF0001 received by procedure INVLOAD. (C D I R)",
        "123503/TESTUSER/INVBATCH",
        "TESTUSER",
        true,
        "C",
    ),
];

/// The queues: `lib/name` and their messages.
type Queues = Vec<(String, Vec<QueuedMessage>)>;

/// The sample message queues.
#[derive(Debug)]
pub struct FixtureMsgs {
    /// Every queue.
    pub queues: Mutex<Queues>,
    /// The commands run, as `user: command`.
    pub ran: Mutex<Vec<String>>,
}

impl Default for FixtureMsgs {
    fn default() -> Self {
        let mine = MESSAGES
            .iter()
            .map(|m| QueuedMessage {
                key: m.0.into(),
                sent: secs(2026, m.1[0], m.1[1], m.1[2], m.1[3], m.1[4]),
                severity: m.2,
                id: m.3.into(),
                text: m.4.into(),
                job: m.5.into(),
                user: m.6.into(),
                inquiry: m.7,
                default: m.8.into(),
                reply: None,
            })
            .collect();
        Self {
            queues: Mutex::new(vec![
                ("QUSRSYS/TESTUSER".into(), mine),
                ("QSYS/QSYSOPR".into(), Vec::new()),
            ]),
            ran: Mutex::new(Vec::new()),
        }
    }
}

/// The value of `KEYWORD(...)` in `command`, quotes removed.
fn parm(command: &str, kw: &str) -> Option<String> {
    let at = command.find(&format!("{kw}("))? + kw.len() + 1;
    let rest = &command[at..];
    let v = if let Some(q) = rest.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = q.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                    continue;
                }
                break;
            }
            out.push(c);
        }
        out
    } else {
        rest[..rest.find(')')?].to_string()
    };
    Some(v)
}

impl FixtureMsgs {
    fn queue_of(&self, q: &str) -> Option<usize> {
        let q = q.trim().to_ascii_uppercase();
        let q = if q.contains('/') {
            q
        } else {
            format!("QUSRSYS/{q}")
        };
        self.queues
            .lock()
            .expect("queues")
            .iter()
            .position(|(n, _)| *n == q)
    }

    fn next_key(all: &Queues) -> String {
        let n = all
            .iter()
            .flat_map(|(_, v)| v.iter().map(QueuedMessage::order))
            .max()
            .unwrap_or(0)
            + 1;
        format!("{n:08X}")
    }

    /// Put a message on queue `q` (`lib/name`), as another job would.
    pub fn arrive(&self, q: &str, mut m: QueuedMessage) -> String {
        let at = self.queue_of(q).expect("queue");
        let mut all = self.queues.lock().expect("queues");
        m.key = Self::next_key(&all);
        let key = m.key.clone();
        all[at].1.push(m);
        key
    }
}

impl Msgs for FixtureMsgs {
    fn own_queue(&self, user: &str) -> (String, String) {
        ("QUSRSYS".into(), user.to_string())
    }

    fn messages(&self, _user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message> {
        let q = format!("{lib}/{name}");
        let all = self.queues.lock().expect("queues");
        match all.iter().find(|(n, _)| *n == q) {
            Some((_, v)) => Ok(v.clone()),
            None => Err(Message::new(
                Kind::Error,
                "CPF2403",
                &format!("Message queue {name} in {lib} not found."),
            )),
        }
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran
            .lock()
            .expect("ran")
            .push(format!("{user}: {command}"));
        let verb = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        let queue = parm(command, "MSGQ").unwrap_or_default();
        let key = parm(command, "MSGKEY").unwrap_or_default();
        match verb.as_str() {
            "SNDMSG" => {
                let to = parm(command, "TOMSGQ")
                    .or_else(|| parm(command, "TOUSR"))
                    .unwrap_or_default();
                if self.queue_of(&to).is_none() {
                    return Err(Message::new(
                        Kind::Error,
                        "CPF2403",
                        &format!("Message queue {to} not found."),
                    ));
                }
                let inquiry = parm(command, "MSGTYPE").as_deref() == Some("*INQ");
                self.arrive(
                    &to,
                    QueuedMessage {
                        sent: super::now(),
                        text: parm(command, "MSG").unwrap_or_default(),
                        job: format!("123463/{user}/ACCESSWEB"),
                        user: user.into(),
                        inquiry,
                        ..QueuedMessage::default()
                    },
                );
                Ok(Message::new(Kind::Done, "", ""))
            }
            "SNDRPYMSG" | "RMVMSG" => {
                let Some(at) = self.queue_of(&queue) else {
                    return Err(Message::new(
                        Kind::Error,
                        "CPF2403",
                        &format!("Message queue {queue} not found."),
                    ));
                };
                let mut all = self.queues.lock().expect("queues");
                let msgs = &mut all[at].1;
                let Some(i) = msgs.iter().position(|m| m.key == key) else {
                    return Err(Message::new(
                        Kind::Error,
                        "CPF2410",
                        &format!("Message key {key} not found in message queue."),
                    ));
                };
                if verb == "RMVMSG" {
                    msgs.remove(i);
                } else {
                    let r = parm(command, "RPY").unwrap_or_default();
                    let r = if r == "*DFT" {
                        msgs[i].default.clone()
                    } else {
                        r
                    };
                    msgs[i].reply = Some(r);
                }
                Ok(Message::new(Kind::Done, "", ""))
            }
            _ => Err(Message::new(Kind::Error, "CPD0030", "Command not found.")),
        }
    }
}
