// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message queues (`QSYSOPR`, the user's own) and the history log `QHST`.
//! Reads are what `DSPMSG` and `DSPLOG` show the user; every change
//! (`SNDMSG`, `SNDRPYMSG`, `RMVMSG`, `CLRMSGQ`) is a CL command run as the
//! user through [`Msgs::run`].

use qsweb::msg::Message;

/// One message on a queue, as `DSPMSG` and its F1 detail show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueuedMessage {
    /// The message key, 8 hex digits (`0000A1F3`).
    pub key: String,
    /// `CPA5305`; empty for a message typed by a user.
    pub id: String,
    /// The severity, 0 to 99.
    pub severity: u8,
    /// An inquiry (it takes a reply).
    pub inquiry: bool,
    /// Sent, seconds since 1970.
    pub sent: u64,
    /// The first-level text, data substituted.
    pub text: String,
    /// The second-level cause, data substituted; empty for none.
    pub cause: String,
    /// The second-level recovery.
    pub recovery: String,
    /// The sending job, `number/user/name`.
    pub job: String,
    /// The sending user.
    pub user: String,
    /// The sending program.
    pub program: String,
    /// An inquiry's valid replies (`C R`), empty for any.
    pub replies: String,
    /// An inquiry's default reply.
    pub default: String,
    /// An inquiry's reply, `None` while it waits for one.
    pub reply: Option<String>,
}

impl QueuedMessage {
    /// An inquiry still waiting for its reply.
    pub fn waiting(&self) -> bool {
        self.inquiry && self.reply.is_none()
    }
}

/// One history log entry (`DSPLOG`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistEntry {
    /// Sent, seconds since 1970.
    pub sent: u64,
    /// `CPF1164`.
    pub id: String,
    /// The severity.
    pub severity: u8,
    /// The job, `number/user/name`.
    pub job: String,
    /// The text, data substituted.
    pub text: String,
}

/// What the message pages need.
pub trait Msgs: Send + Sync {
    /// The queues the operator's view offers (`lib`, `name`): the
    /// system's, then the users'.
    fn queues(&self, user: &str) -> Vec<(String, String)>;
    /// The message queue of `user`'s profile (`MSGQ`), qualified.
    fn own_queue(&self, user: &str) -> (String, String);
    /// The messages on queue `lib/name`, oldest first, replies left out
    /// (an inquiry carries its own). `Err` is the escape message
    /// (`CPF2403` no such queue, `CPF2217` not authorized).
    fn messages(&self, user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message>;
    /// The history log, oldest first.
    fn history(&self, user: &str) -> Vec<HistEntry>;
    /// Run CL `command` as `user`: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}
