// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Program messages: the queue every call-stack entry has, the job's
//! external queue and `QSYSOPR`, and sending, receiving and removing.
//! A message keeps its identifier, file and data; its text is
//! substituted when it is sent.

use qir::MsgType;

/// A message on a queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// The key, unique in the job.
    pub key: u32,
    /// `AAAnnnn`.
    pub msgid: String,
    /// Message file.
    pub msgf: String,
    /// Message file library as given.
    pub msgf_lib: String,
    /// Type.
    pub msg_type: MsgType,
    /// Message data.
    pub data: Vec<u8>,
    /// First-level text, substituted.
    pub text: String,
    /// Second-level text, substituted.
    pub help: String,
    /// Severity.
    pub severity: u16,
    /// Call-stack depth of the sender.
    pub from_depth: u32,
    /// Received already (`*OLD`).
    pub received: bool,
    /// The reply, for an inquiry that got one.
    pub reply: Option<String>,
}

/// `RMVMSG CLEAR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearMode {
    /// `*ALL`.
    All,
    /// `*OLD`: received messages.
    Old,
    /// `*NEW`: messages not yet received.
    New,
    /// `*KEEPUNANS`: everything but unanswered inquiries.
    KeepUnans,
}

impl ClearMode {
    /// From the code generated code passes.
    #[must_use]
    pub fn from_code(c: i64) -> Self {
        match c {
            1 => Self::Old,
            2 => Self::New,
            3 => Self::KeepUnans,
            _ => Self::All,
        }
    }
}

/// A message queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MsgQueue {
    /// Messages, oldest first.
    pub msgs: Vec<Message>,
}

impl MsgQueue {
    /// Append.
    pub fn push(&mut self, m: Message) {
        self.msgs.push(m);
    }

    /// The oldest message of a type (any when `None`), or the one with
    /// a key; marked received, or removed when `rmv`.
    pub fn receive(&mut self, ty: Option<MsgType>, key: Option<u32>, rmv: bool) -> Option<Message> {
        let i = self.msgs.iter().position(|m| {
            key.is_none_or(|k| m.key == k)
                && ty.is_none_or(|t| t == MsgType::Any || m.msg_type == t)
        })?;
        if rmv {
            Some(self.msgs.remove(i))
        } else {
            self.msgs[i].received = true;
            Some(self.msgs[i].clone())
        }
    }

    /// Remove one message by key.
    pub fn remove(&mut self, key: u32) -> bool {
        let before = self.msgs.len();
        self.msgs.retain(|m| m.key != key);
        before != self.msgs.len()
    }

    /// Remove messages by mode; returns how many.
    pub fn clear(&mut self, mode: ClearMode) -> usize {
        let before = self.msgs.len();
        self.msgs.retain(|m| match mode {
            ClearMode::All => false,
            ClearMode::Old => !m.received,
            ClearMode::New => m.received,
            ClearMode::KeepUnans => m.msg_type == MsgType::Inq && m.reply.is_none(),
        });
        before - self.msgs.len()
    }
}

/// Which queue a message goes to or comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueRef {
    /// The current call-stack entry.
    Same,
    /// Its caller.
    Prv,
    /// The nearest control boundary at or below the current entry.
    CtlBdy,
    /// The job's external queue.
    Ext,
    /// The system operator's queue.
    SysOpr,
    /// `n` entries up the stack from the current one.
    Up(u32),
}

impl QueueRef {
    /// From the kind and count generated code passes.
    #[must_use]
    pub fn from_code(kind: i64, n: i64) -> Self {
        match kind {
            1 => Self::Prv,
            2 => Self::CtlBdy,
            3 => Self::Ext,
            4 => Self::SysOpr,
            5 => Self::Up(u32::try_from(n).unwrap_or(0)),
            _ => Self::Same,
        }
    }
}

/// The message type for a code in declaration order of [`MsgType`].
#[must_use]
pub fn msg_type_of(code: i64) -> Option<MsgType> {
    Some(match code {
        0 => MsgType::Info,
        1 => MsgType::Comp,
        2 => MsgType::Diag,
        3 => MsgType::Escape,
        4 => MsgType::Status,
        5 => MsgType::Notify,
        6 => MsgType::Rqs,
        7 => MsgType::Inq,
        8 => MsgType::Rpy,
        9 => MsgType::Scope,
        10 => MsgType::Any,
        _ => return None,
    })
}
