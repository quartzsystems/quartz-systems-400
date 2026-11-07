// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages (handoff step 16): what `DSPMSG` shows of the user's own
//! queue, `QSYSOPR` or any queue the user may read; every change
//! (`SNDMSG`, `SNDRPYMSG`, `RMVMSG`) is a CL command run as the
//! signed-on user through [`Msgs::run`].

use qsweb::msg::Message;

/// One message on a queue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueuedMessage {
    /// The message key, 8 hex digits (`0000A1F3`).
    pub key: String,
    /// `CPA0702`; empty for a message a user typed.
    pub id: String,
    /// The severity, 0 to 99.
    pub severity: u8,
    /// An inquiry (it takes a reply).
    pub inquiry: bool,
    /// Sent, seconds since 1970.
    pub sent: u64,
    /// The first-level text, data substituted.
    pub text: String,
    /// The sending job, `number/user/name`.
    pub job: String,
    /// The sending user.
    pub user: String,
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

    /// The key as a number, for ordering (keys grow as messages arrive).
    pub fn order(&self) -> u64 {
        u64::from_str_radix(self.key.trim(), 16).unwrap_or(0)
    }

    /// The replies an inquiry names at the end of its text, `(C D I R)`
    /// as `C D I R`; empty when it names none.
    pub fn replies(&self) -> String {
        let t = self.text.trim_end();
        let Some(open) = t.strip_suffix(')').and_then(|s| s.rfind('(').map(|i| &s[i + 1..])) else {
            return String::new();
        };
        let ok = !open.is_empty()
            && open.split(' ').all(|w| !w.is_empty() && w.len() <= 4 && w.chars().all(|c| c.is_ascii_alphanumeric()));
        if ok { open.to_string() } else { String::new() }
    }
}

/// What the message pages need.
pub trait Msgs: Send + Sync {
    /// The message queue of `user`'s profile (`MSGQ`), as `(lib, name)`.
    fn own_queue(&self, user: &str) -> (String, String);
    /// The messages on queue `lib/name`, oldest first, replies left out
    /// (an inquiry carries its own). `Err` is the escape message
    /// (`CPF2403` no such queue, `CPF2217` not authorized).
    fn messages(&self, user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message>;
    /// Run CL `command` as `user`: its completion or escape message.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_from_text() {
        let m = |t: &str| QueuedMessage {
            text: t.into(),
            ..QueuedMessage::default()
        };
        assert_eq!(m("CPF0001 received by procedure INVLOAD. (C D I R)").replies(), "C D I R");
        assert_eq!(m("Load the tape (volume TAPE01).").replies(), "");
        assert_eq!(m("No replies").replies(), "");
    }
}
