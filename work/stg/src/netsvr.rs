// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer over the link (`docs/NETSERVER.md`): the `QZLS*` APIs'
//! calls (`netsvr_api`, tag 1500) and the session table the
//! `QZLSSERVER` and `QZLSFILE` jobs report into (`netsvr`, tag 1501,
//! one [`NsOp`] and its [`NsReply`]). Only jobs named `QZLSSERVER` or
//! `QZLSFILE` may change the table.

#![allow(clippy::doc_markdown)] // NetServer, NetBIOS

use ipc::{wire_enum, wire_struct};

/// A tree connection as the table holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NsConn {
    /// Its identifier.
    pub id: u32,
    /// The share, upper case.
    pub share: String,
    /// 0 disk, 1 print, 3 IPC.
    pub kind: i32,
    /// Files open through it.
    pub opens: u32,
    /// When it was made (seconds since the epoch).
    pub since: u64,
}

wire_struct!(NsConn {
    id,
    share,
    kind,
    opens,
    since
});

/// A session as the table holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NsSession {
    /// Its identifier (0 when registering).
    pub id: u64,
    /// The client's NetBIOS name or address.
    pub workstation: String,
    /// The client's full name or address.
    pub expanded: String,
    /// The profile signed on.
    pub user: String,
    /// The job serving it (filled by the service on register).
    pub job: String,
    /// Signed on as the guest.
    pub guest: bool,
    /// When it started.
    pub since: u64,
    /// Its last request.
    pub last_active: u64,
    /// `QZLSENSS` asked it to end.
    pub ending: bool,
    /// Its tree connections.
    pub connections: Vec<NsConn>,
}

wire_struct!(NsSession {
    id,
    workstation,
    expanded,
    user,
    job,
    guest,
    since,
    last_active,
    ending,
    connections
});

/// One session table request.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)] // the fields are the table's
pub enum NsOp {
    /// The server started: the table and counters from zero.
    Started,
    /// A session signed on; the reply is its identifier.
    Register { session: NsSession },
    /// A session ended; `idle` when the idle time-out ended it.
    Unregister { id: u64, idle: bool },
    /// A tree connection; the reply is its identifier.
    Connect { id: u64, share: String, kind: i32 },
    /// A tree connection ended.
    Disconnect { id: u64, conn: u32 },
    /// Files open through a connection are now `opens`.
    Opens { id: u64, conn: u32, opens: u32 },
    /// A session made a request.
    Touch { id: u64 },
    /// Add `n` to counter `counter` (`netsvrcfg::Counter` in order:
    /// 0 opens, 1 print jobs, 2 auto disconnects, 3 duplicate sessions,
    /// 4 session errors, 5 password violations, 6 unknown users, 7
    /// bytes sent, 8 bytes received, 9 a response of `n` ms).
    Count { counter: u8, n: u64 },
    /// Every session.
    List,
    /// Whether session `id` was asked to end.
    Ending { id: u64 },
    /// The job ended: its sessions go.
    JobEnded,
    /// Both configuration files (`QAZLSCFG`, `QAZLSSHR`), created with
    /// the shipped share when missing.
    Config,
    /// Start `QZLSSERVER` (`STRTCPSVR SERVER(*NETSVR)`, `STRTCP`'s
    /// autostart), as `QZLSSTRS` does; `reset` applies the pending
    /// attributes. An autostart start does nothing unless `*NETSVR`
    /// autostarts.
    Start { reset: bool, autostart: bool },
    /// End `QZLSSERVER` and every `QZLSFILE`'s connections, as
    /// `QZLSENDS` does.
    End,
    /// Whether `QZLSSERVER` is active and whether it was asked to end.
    State,
    /// The calling job's routes for reconnects: its relay identifier,
    /// the `ClientGuid`s it serves or holds opens for, and the client
    /// addresses of its durable opens. Empty lists withdraw it.
    Route { relay: Vec<u8>, guids: Vec<Vec<u8>>, peers: Vec<String> },
    /// The job a new connection goes to: the one that registered `guid`
    /// (16 bytes, or empty), else `peer`; an empty relay for none.
    Lookup { guid: Vec<u8>, peer: String },
    /// [`NsOp::Connect`] within the share's maximum users, counted
    /// across jobs: the connection's identifier, or `Refused`.
    Admit { id: u64, share: String, kind: i32 },
}

wire_enum!(NsOp {
    0 => Started,
    1 => Register { session },
    2 => Unregister { id, idle },
    3 => Connect { id, share, kind },
    4 => Disconnect { id, conn },
    5 => Opens { id, conn, opens },
    6 => Touch { id },
    7 => Count { counter, n },
    8 => List,
    9 => Ending { id },
    10 => JobEnded,
    11 => Config,
    12 => Start { reset, autostart },
    13 => End,
    14 => State,
    15 => Route { relay, guids, peers },
    16 => Lookup { guid, peer },
    17 => Admit { id, share, kind },
});

/// The answer to an [`NsOp`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum NsReply {
    /// Done.
    Done,
    /// A new session's identifier.
    Session { id: u64 },
    /// A new connection's identifier.
    Conn { id: u32 },
    /// Every session.
    Sessions { list: Vec<NsSession> },
    /// A yes or no.
    Flag { yes: bool },
    /// The configuration files.
    Files { cfg: Vec<u8>, shr: Vec<u8> },
    /// Refused or failed.
    Error { text: String },
    /// Refused with a message: its identifier and first-level text.
    Refused { msgid: String, text: String },
    /// `QZLSSERVER`'s state.
    State { active: bool, ending: bool },
    /// The relay identifier of the job a connection goes to; empty for
    /// a new prestart job.
    Target { relay: Vec<u8> },
}

wire_enum!(NsReply {
    0 => Done,
    1 => Session { id },
    2 => Conn { id },
    3 => Sessions { list },
    4 => Flag { yes },
    5 => Files { cfg, shr },
    6 => Error { text },
    7 => Refused { msgid, text },
    8 => State { active, ending },
    9 => Target { relay },
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_crosses_the_wire_whole() {
        let s = NsSession {
            id: 7,
            workstation: "PC1".into(),
            expanded: "pc1.example.com".into(),
            user: "BOB".into(),
            job: "000001/QUSER/QZLSFILE".into(),
            guest: false,
            since: 5,
            last_active: 6,
            ending: true,
            connections: vec![NsConn {
                id: 1,
                share: "QIBM".into(),
                kind: 0,
                opens: 2,
                since: 5,
            }],
        };
        let op = NsOp::Register { session: s.clone() };
        let back: NsOp = ipc::decode(&ipc::encode(&op)).expect("decodes");
        assert_eq!(back, op);
        let r = NsReply::Sessions { list: vec![s] };
        let back: NsReply = ipc::decode(&ipc::encode(&r)).expect("decodes");
        assert_eq!(back, r);
    }
}
