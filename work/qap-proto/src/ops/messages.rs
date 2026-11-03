// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Messages` (version 6): message queues, replies and watches. Keys
//! are 4 bytes. Flags are `bool`s: one byte, 0 or 1, any other value
//! does not decode.

use ipc::{wire_enum, wire_struct};

use crate::types::{JobId, ObjectName};

/// A message key: unique within its queue while the message exists.
pub type Key = [u8; 4];

/// A message's type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MessageType {
    /// Information.
    #[default]
    Informational,
    /// A question waiting for a reply.
    Inquiry,
    /// The reply to an inquiry or a sender's copy.
    Reply,
    /// A function completed.
    Completion,
    /// A problem found.
    Diagnostic,
    /// A function ended in error.
    Escape,
    /// A condition a program may handle.
    Notify,
    /// The sender's copy of an inquiry, on the queue its reply goes to.
    SenderCopy,
}

wire_enum!(MessageType {
    0 => Informational,
    1 => Inquiry,
    2 => Reply,
    3 => Completion,
    4 => Diagnostic,
    5 => Escape,
    6 => Notify,
    7 => SenderCopy,
});

impl MessageType {
    /// Every type, in value order.
    pub const ALL: [MessageType; 8] = [
        MessageType::Informational,
        MessageType::Inquiry,
        MessageType::Reply,
        MessageType::Completion,
        MessageType::Diagnostic,
        MessageType::Escape,
        MessageType::Notify,
        MessageType::SenderCopy,
    ];
}

/// A message on a queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueuedMessage {
    /// Its key on the queue.
    pub key: Key,
    /// Its identifier; empty for an impromptu message.
    pub id: String,
    /// Its type.
    pub kind: MessageType,
    /// Its severity.
    pub severity: u8,
    /// First-level text, in the connection job's language.
    pub text: String,
    /// Second-level text.
    pub help: String,
    /// When it was sent, milliseconds since 1970-01-01 UTC.
    pub sent: i64,
    /// The sending job's user.
    pub from_user: String,
    /// The sending job.
    pub from_job: JobId,
    /// An inquiry waiting for its reply.
    pub needs_reply: bool,
    /// An inquiry's or sender's copy's reply, once it has one.
    pub reply: Option<String>,
    /// A reply's: the key of the message it answers on the same queue.
    pub answers: Option<Key>,
}

wire_struct!(QueuedMessage {
    key,
    id,
    kind,
    severity,
    text,
    help,
    sent,
    from_user,
    from_job,
    needs_reply,
    reply,
    answers
});

/// A message queue.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum Queue {
    /// The signed-on profile's.
    #[default]
    Own,
    /// `QSYSOPR`.
    SystemOperator,
    /// Any other, by name.
    Named {
        /// The queue.
        queue: ObjectName,
    },
}

wire_enum!(Queue {
    0 => Own,
    1 => SystemOperator,
    2 => Named { queue },
});

/// Where `SendMessage` sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipient {
    /// A profile's message queue.
    User {
        /// The profile.
        profile: String,
    },
    /// A queue.
    Queue {
        /// The queue.
        queue: Queue,
    },
}

impl Default for Recipient {
    fn default() -> Self {
        Self::Queue { queue: Queue::Own }
    }
}

wire_enum!(Recipient {
    0 => User { profile },
    1 => Queue { queue },
});

/// `ListMessages`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListMessages {
    /// The queue.
    pub queue: Queue,
    /// Strictly newest first, else oldest first.
    pub newest_first: bool,
    /// The first message, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListMessages {
    queue,
    newest_first,
    start,
    max
});

/// `ListMessages`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Messages {
    /// Messages on the queue.
    pub total: u32,
    /// Those from `start`.
    pub messages: Vec<QueuedMessage>,
}

wire_struct!(Messages { total, messages });

/// `ReplyMessage`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplyMessage {
    /// The inquiry's queue.
    pub queue: Queue,
    /// The inquiry's key.
    pub key: Key,
    /// The reply; empty for the default reply.
    pub reply: String,
}

wire_struct!(ReplyMessage { queue, key, reply });

/// `RemoveMessages`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoveMessages {
    /// The queue.
    pub queue: Queue,
    /// The messages; all or none are removed.
    pub keys: Vec<Key>,
}

wire_struct!(RemoveMessages { queue, keys });

/// `SendMessage`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendMessage {
    /// Where to.
    pub to: Recipient,
    /// The text.
    pub text: String,
    /// An inquiry, its reply coming to the sender's own queue.
    pub inquiry: bool,
}

wire_struct!(SendMessage { to, text, inquiry });

/// `SendMessage`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sent {
    /// An inquiry's: the key its reply's `answers` will name.
    pub key: Option<Key>,
}

wire_struct!(Sent { key });

/// The request of `WatchQueue` and `UnwatchQueue`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueRef {
    /// The queue.
    pub queue: Queue,
}

wire_struct!(QueueRef { queue });

/// `MessageArrived`'s body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageArrived {
    /// The watched queue.
    pub queue: Queue,
    /// The message.
    pub message: QueuedMessage,
}

wire_struct!(MessageArrived { queue, message });
