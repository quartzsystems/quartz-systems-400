// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Messages` service in the connection job: the storage service
//! answers as the message commands do (`stgsvc/qap/messages.rs`); the
//! job gives each message the client's form, its text and second-level
//! text from its message file in the job's language (an impromptu
//! message's as it was sent).

use std::collections::HashMap;

use msgf::MsgDesc;
use qap_proto::ops::messages::{MessageType, QueuedMessage};
use qap_proto::types::JobId;
use stg::{Client, Link};
use wm::obj::msgq::{MsgKind, QMsg, Status};

/// A message type as QAP names it.
fn kind_of(k: MsgKind) -> MessageType {
    match k {
        MsgKind::Info => MessageType::Informational,
        MsgKind::Inq => MessageType::Inquiry,
        MsgKind::Rpy => MessageType::Reply,
        MsgKind::Comp => MessageType::Completion,
        MsgKind::Diag => MessageType::Diagnostic,
        MsgKind::Escape => MessageType::Escape,
        MsgKind::Notify => MessageType::Notify,
        MsgKind::Copy => MessageType::SenderCopy,
    }
}

/// Descriptions looked up for one answer, by identifier and file.
#[derive(Default)]
pub struct Texts(HashMap<(String, String, String), Option<MsgDesc>>);

impl Texts {
    /// `m`'s text and second-level text in the job's language.
    fn of(&mut self, link: &mut Link, m: &QMsg) -> (String, String) {
        if m.msgid.is_empty() {
            return (m.text.clone(), m.help.clone());
        }
        let lib = if m.msgf.1.trim().is_empty() {
            "*LIBL".to_string()
        } else {
            m.msgf.1.trim().to_string()
        };
        let file = if m.msgf.0.trim().is_empty() {
            sysapi::param::file_of(&m.msgid).to_string()
        } else {
            m.msgf.0.trim().to_string()
        };
        let k = (m.msgid.clone(), file, lib);
        let d = self
            .0
            .entry(k.clone())
            .or_insert_with(|| link.with(|l| l.describe(k.0, k.1, k.2)));
        match d {
            Some(d) => (d.format(&m.data), d.format_help(&m.data)),
            None => (m.text.clone(), m.help.clone()),
        }
    }
}

/// `m` as the client sees it.
pub fn queued(link: &mut Link, texts: &mut Texts, m: &QMsg) -> QueuedMessage {
    let (text, help) = texts.of(link, m);
    QueuedMessage {
        key: m.key.to_be_bytes(),
        id: m.msgid.clone(),
        kind: kind_of(m.kind),
        severity: m.severity,
        text,
        help,
        sent: i64::try_from(m.time / 1000).unwrap_or(i64::MAX),
        from_user: m.job.1.clone(),
        from_job: JobId {
            name: m.job.0.clone(),
            user: m.job.1.clone(),
            number: format!("{:06}", m.job.2),
        },
        needs_reply: m.kind == MsgKind::Inq && m.status == Status::Unanswered,
        reply: (matches!(m.kind, MsgKind::Inq | MsgKind::Copy) && m.status == Status::Answered)
            .then(|| m.reply.clone()),
        answers: (m.kind == MsgKind::Rpy).then(|| m.reply_key.to_be_bytes()),
    }
}
