// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server over the link (docs/DIRSRV.md §4): requests
//! 1560-1567 from an instance job (`QUSRDIR`, user `QDIRSRV`) to the
//! service, which holds the entries, the schema and every session.
//!
//! LDAP messages travel as their BER bytes (`ldap::msg` encodes and
//! decodes them at both ends): a request is the message the client sent;
//! each answer's `replies` are whole messages carrying that message's
//! ID, ready to write to the connection in order. Unbind and abandon are
//! the job's, told to the service with `dir_close`.

use ipc::{wire_enum, wire_struct};

/// What the job knows of a connection: sent at `dir_open` and with each
/// bind (`StartTLS` changes it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirConn {
    /// The client's address and port, as `NETSTAT` shows it.
    pub client: String,
    /// The connection is under TLS (port 636, or after `StartTLS`).
    pub tls: bool,
    /// The TLS client certificate's subject DN, when one was presented.
    pub client_subject: Option<String>,
}

wire_struct!(DirConn {
    client,
    tls,
    client_subject,
});

/// What the instance job runs by: `ibmslapd.conf`'s values for the
/// listeners and the connections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirSettings {
    /// The instance's name.
    pub instance: String,
    /// `ibm-slapdPort`.
    pub port: u16,
    /// `ibm-slapdSecurePort`.
    pub secure_port: u16,
    /// `ibm-slapdSecurity`: `none`, `SSL`, `TLS` or `SSLTLS`.
    pub security: String,
    /// `ibm-slapdSizeLimit` (0: none); the service applies it too.
    pub size_limit: u32,
    /// `ibm-slapdTimeLimit`, seconds (0: none): the job ends a search
    /// that runs longer for anyone but an administrator.
    pub time_limit: u32,
    /// `ibm-slapdIdleTimeOut`, seconds (0: none).
    pub idle_timeout: u32,
    /// `ibm-slapdWriteTimeout`, seconds (0: none).
    pub write_timeout: u32,
    /// The certificate store's application ID for the server's
    /// certificate (`QS400_DIRECTORY_SERVER`).
    pub app_id: String,
    /// The largest request the job takes, bytes.
    pub max_message: u32,
}

wire_struct!(DirSettings {
    instance,
    port,
    secure_port,
    security,
    size_limit,
    time_limit,
    idle_timeout,
    write_timeout,
    app_id,
    max_message,
});

/// What `dir_open` answers: the connection's session and the settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirOpened {
    /// The session ID every later request names.
    pub session: u64,
    /// The instance's settings.
    pub settings: DirSettings,
}

wire_struct!(DirOpened { session, settings });

/// How much one `dir_search` or `dir_next` may answer. The service caps
/// both so the single-threaded service never runs one search long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirBudget {
    /// Entries and references.
    pub entries: u32,
    /// Bytes of entries, roughly as encoded (at least one entry goes).
    pub bytes: u32,
}

impl Default for DirBudget {
    fn default() -> Self {
        Self {
            entries: 100,
            bytes: 256 * 1024,
        }
    }
}

wire_struct!(DirBudget { entries, bytes });

/// Where a request leaves its operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DirState {
    /// Answered in full (a search's done is the last reply).
    #[default]
    Done,
    /// More entries: ask `dir_next` with this cursor.
    More {
        /// The cursor.
        cursor: u64,
    },
    /// A page of paged results is over; the next page comes with the
    /// client's next search request (its cookie names the cursor).
    Paused {
        /// The cursor.
        cursor: u64,
    },
    /// A persistent search's initial entries are over; its changes come
    /// from `dir_changes` until it is abandoned.
    Watching {
        /// The watch.
        watch: u64,
    },
}

wire_enum!(DirState {
    0 => Done,
    1 => More { cursor },
    2 => Paused { cursor },
    3 => Watching { watch },
});

/// One request's answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirAnswer {
    /// The messages to send, in order, each BER-encoded with the
    /// request's message ID.
    pub replies: Vec<Vec<u8>>,
    /// What follows.
    pub state: DirState,
    /// `StartTLS` succeeded: start the handshake after sending the
    /// reply (without a certificate the job answers 2 itself and never
    /// sends the request).
    pub start_tls: bool,
    /// After a bind: the DN the connection is bound as, `None` for
    /// anonymous (a failed bind leaves it anonymous).
    pub bound: Option<String>,
}

wire_struct!(DirAnswer {
    replies,
    state,
    start_tls,
    bound,
});

/// What `dir_close` ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirClose {
    /// The search of message `id` (abandon, a time limit the job
    /// enforced, a paged search the client ended): its cursor or watch.
    Message {
        /// The search request's message ID.
        id: i32,
    },
    /// The connection ended (unbind, a close, an idle time-out): the
    /// session and everything it holds.
    Session,
}

wire_enum!(DirClose {
    0 => Message { id },
    1 => Session,
});

/// The job's connection counts for `cn=monitor` (`dir_conns`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirConns {
    /// Open now.
    pub current: u64,
    /// Accepted since the job started.
    pub total: u64,
    /// Of those, the ones that started TLS with `StartTLS`.
    pub tls: u64,
    /// Of those, the ones accepted on the secure port.
    pub ssl: u64,
}

wire_struct!(DirConns {
    current,
    total,
    tls,
    ssl,
});

/// What `dir_changes` answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirChanges {
    /// The entries the session's persistent searches saw, each a
    /// BER-encoded search result entry with its search's message ID.
    pub replies: Vec<Vec<u8>>,
    /// The message IDs of persistent searches that no longer exist in
    /// the service: the job ends them.
    pub ended: Vec<i32>,
}

wire_struct!(DirChanges { replies, ended });

/// A request refused: the message ID (`GLDxxxx` or `CPFxxxx`), its
/// substitution values and Quartz's text, for the job log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirError {
    /// The message ID.
    pub msgid: String,
    /// Its substitution values, in its format's order.
    pub vars: Vec<String>,
    /// The text.
    pub text: String,
}

wire_struct!(DirError { msgid, vars, text });

impl DirError {
    /// An error with `msgid`, `vars` and `text`.
    #[must_use]
    pub fn new(msgid: &str, vars: Vec<String>, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            vars,
            text: text.into(),
        }
    }
}

impl std::fmt::Display for DirError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.msgid, self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipc::Wire;

    fn round<T: Wire + PartialEq + std::fmt::Debug>(v: &T) {
        let mut b = Vec::new();
        v.put(&mut b);
        let mut d = ipc::Dec::new(&b);
        assert_eq!(&T::get(&mut d).unwrap(), v);
    }

    #[test]
    fn round_trips() {
        round(&DirConn {
            client: "10.0.0.9:51234".into(),
            tls: true,
            client_subject: Some("cn=Ann,o=Acme".into()),
        });
        round(&DirOpened {
            session: 7,
            settings: DirSettings {
                instance: "QUSRDIR".into(),
                port: 389,
                secure_port: 636,
                security: "none".into(),
                size_limit: 500,
                time_limit: 900,
                idle_timeout: 300,
                write_timeout: 120,
                app_id: "QS400_DIRECTORY_SERVER".into(),
                max_message: 16 << 20,
            },
        });
        round(&DirBudget::default());
        for s in [
            DirState::Done,
            DirState::More { cursor: 3 },
            DirState::Paused { cursor: 4 },
            DirState::Watching { watch: 5 },
        ] {
            round(&DirAnswer {
                replies: vec![vec![0x30, 0x0c], vec![]],
                state: s,
                start_tls: true,
                bound: Some("cn=Ann".into()),
            });
        }
        round(&DirClose::Message { id: -1 });
        round(&DirClose::Session);
        round(&DirConns {
            current: 1,
            total: 9,
            tls: 2,
            ssl: 3,
        });
        round(&DirChanges {
            replies: vec![vec![1, 2]],
            ended: vec![4],
        });
        round(&DirError::new(
            "GLD0203",
            vec!["X".into()],
            "no such instance",
        ));
    }
}
