// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Machine conditions as exceptions: the message identifier, the RPG
//! status the language runtime reports for it, and the class a
//! `monitor` selector tests.

pub use codegen::Cond;
use codegen::rt::{selector, unpack_msgid};

/// A pending exception: an escape message sent to a call-stack entry,
/// from a machine condition or from `sndpgmmsg`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exception {
    /// The machine condition, when one raised it.
    pub cond: Option<Cond>,
    /// `AAAnnnn`.
    pub msgid: String,
    /// Message file.
    pub msgf: String,
    /// Message data.
    pub data: Vec<u8>,
    /// The five-digit RPG status the condition maps to; 0 when none.
    pub status: u16,
    /// Call-stack depth of the entry it was sent to.
    pub depth: u32,
    /// Key of the escape message that carries it.
    pub key: u32,
}

impl Exception {
    /// An exception for a condition raised at a depth.
    #[must_use]
    pub fn new(cond: Cond, depth: u32) -> Self {
        Self {
            cond: Some(cond),
            msgid: cond.msgid(),
            msgf: "QCPFMSG".into(),
            data: Vec::new(),
            status: status_of(cond),
            depth,
            key: 0,
        }
    }

    /// An exception for an escape message sent to an entry.
    #[must_use]
    pub fn message(msgid: &str, msgf: &str, data: Vec<u8>, depth: u32) -> Self {
        let cond = msgid
            .strip_prefix("MCH")
            .and_then(|n| n.parse::<u32>().ok())
            .and_then(Cond::from_code);
        // The RPG statuses of the messages the runtime itself sends: a
        // cancelled callee is 202, a function check 9999.
        let status = match msgid {
            "CEE9901" | "CEE9902" => 202,
            "CPF9999" => 9999,
            _ => cond.map_or(0, status_of),
        };
        Self {
            cond,
            msgid: msgid.into(),
            msgf: msgf.into(),
            data,
            status,
            depth,
            key: 0,
        }
    }

    /// Whether a `monitor` selector, as `rt::selector` encodes it,
    /// takes this exception.
    #[must_use]
    pub fn matches(&self, kind: i64, a: i64, b: i64) -> bool {
        match kind {
            selector::ALL => true,
            selector::PROGRAM => !self.is_file_error(),
            selector::FILE => self.is_file_error(),
            selector::STATUS => i64::from(self.status) >= a && i64::from(self.status) <= b,
            selector::MSGID => {
                let id = unpack_msgid(a);
                match id.strip_suffix('*') {
                    Some(prefix) => self.msgid.starts_with(prefix),
                    None => self.msgid == id,
                }
            }
            _ => false,
        }
    }

    /// Whether a file operation raised it: the `CPF4` and `CPF5`
    /// families.
    #[must_use]
    pub fn is_file_error(&self) -> bool {
        self.msgid.starts_with("CPF4") || self.msgid.starts_with("CPF5")
    }
}

/// The RPG status code of a machine condition, as the RPG runtime maps
/// it (`*STATUS` and the `ON-ERROR` ranges).
#[must_use]
pub fn status_of(c: Cond) -> u16 {
    match c {
        Cond::Mch1211 => 102,
        Cond::Mch1210 => 103,
        Cond::Mch0603 => 121,
        Cond::Mch1202 => 907,
        Cond::Mch1602 => 112,
        Cond::Mch0601
        | Cond::Mch0602
        | Cond::Mch3401
        | Cond::Mch3402
        | Cond::Mch3601
        | Cond::Mch3602
        | Cond::Mch4431 => 222,
    }
}
