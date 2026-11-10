// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What one `QZLSFILE` reports into the service's session table (1511):
//! its sessions, their tree connections and open files. A tree connect
//! is admitted through the table (`NsOp::Admit`, the share's maximum
//! users counted across jobs); the rest follows the server's own view
//! ([`netserver::NetServer::sessions`]) once a second.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use netserver::{SessionView, TreeAdmission, TreeRequest};
use stg::Client;
use stg::netsvr::{NsOp, NsReply, NsSession};

use super::svc::Svc;

/// One session as reported: its table identifier and its trees'.
#[derive(Debug, Default)]
struct Reported {
    id: u64,
    /// The library's tree identifier to the table's and the opens last
    /// reported.
    trees: HashMap<u32, (u32, u32)>,
}

/// The job's sessions as the table has them, by the library's
/// SessionId.
#[derive(Default)]
pub struct Table {
    svc: Option<Arc<Svc>>,
    sessions: Mutex<HashMap<u64, Reported>>,
    /// The workstation name a NetBIOS session request gave.
    workstation: Mutex<String>,
}

/// The table's share type of the library's: 0 disk, 1 print, 3 IPC.
fn kind(k: u8) -> i32 {
    match k {
        2 => 3,
        3 => 1,
        _ => 0,
    }
}

fn ip(peer: Option<SocketAddr>) -> String {
    peer.map(|p| p.ip().to_string()).unwrap_or_default()
}

impl Table {
    /// Reporting through `svc`.
    pub fn new(svc: Arc<Svc>) -> Self {
        Self {
            svc: Some(svc),
            ..Self::default()
        }
    }

    /// The client's NetBIOS name (port 139), for the next sessions.
    pub fn set_workstation(&self, name: &str) {
        name.clone_into(
            &mut self
                .workstation
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    fn ask(&self, op: NsOp) -> Option<NsReply> {
        let svc = self.svc.as_ref()?;
        svc.call(move |l| l.borrow_mut().with(|c| c.netsvr(op)))
    }

    /// The table's identifier of session `sid`, registering it first.
    fn register(
        &self,
        map: &mut HashMap<u64, Reported>,
        sid: u64,
        user: &str,
        guest: bool,
        peer: Option<SocketAddr>,
    ) -> Option<u64> {
        if let Some(r) = map.get(&sid) {
            return Some(r.id);
        }
        let addr = ip(peer);
        let ws = self
            .workstation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let session = NsSession {
            workstation: if ws.is_empty() { addr.clone() } else { ws },
            expanded: addr,
            user: user.to_string(),
            guest,
            ..NsSession::default()
        };
        match self.ask(NsOp::Register { session }) {
            Some(NsReply::Session { id }) => {
                map.insert(
                    sid,
                    Reported {
                        id,
                        trees: HashMap::new(),
                    },
                );
                Some(id)
            }
            other => {
                eprintln!("qsjob: QZLSFILE: session not registered: {other:?}");
                None
            }
        }
    }

    /// Bring the table to the server's view `now`; the table's
    /// identifiers of sessions `QZLSENSS` marked to end, by the library's.
    pub fn sync(&self, now: &[SessionView]) -> Vec<u64> {
        let mut map = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let gone: Vec<u64> = map
            .keys()
            .filter(|k| !now.iter().any(|s| s.session_id == **k))
            .copied()
            .collect();
        for sid in gone {
            if let Some(r) = map.remove(&sid) {
                self.ask(NsOp::Unregister {
                    id: r.id,
                    idle: false,
                });
            }
        }
        let mut ending = Vec::new();
        for s in now {
            let Some(id) = self.register(&mut map, s.session_id, &s.user, s.guest, s.peer) else {
                continue;
            };
            let r = map.get_mut(&s.session_id).expect("registered");
            let old: Vec<u32> = r
                .trees
                .keys()
                .filter(|t| !s.trees.iter().any(|v| v.tree_id == **t))
                .copied()
                .collect();
            for t in old {
                if let Some((conn, _)) = r.trees.remove(&t) {
                    self.ask(NsOp::Disconnect { id, conn });
                }
            }
            for t in &s.trees {
                match r.trees.get_mut(&t.tree_id) {
                    Some((conn, opens)) if *opens != t.opens => {
                        *opens = t.opens;
                        self.ask(NsOp::Opens {
                            id,
                            conn: *conn,
                            opens: t.opens,
                        });
                    }
                    Some(_) => {}
                    // A tree admitted before its session was seen here.
                    None => {
                        if let Some(NsReply::Conn { id: conn }) = self.ask(NsOp::Connect {
                            id,
                            share: t.share.clone(),
                            kind: kind(t.kind),
                        }) {
                            r.trees.insert(t.tree_id, (conn, 0));
                        }
                    }
                }
            }
            if matches!(
                self.ask(NsOp::Ending { id }),
                Some(NsReply::Flag { yes: true })
            ) {
                ending.push(s.session_id);
            }
        }
        ending
    }

    /// Session `sid` was ended (`QZLSENSS`): out of the table.
    pub fn ended(&self, sid: u64) {
        let r = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&sid);
        if let Some(r) = r {
            self.ask(NsOp::Unregister {
                id: r.id,
                idle: false,
            });
        }
    }

    /// Every session out of the table (the job's client is gone).
    pub fn clear(&self) {
        let all: Vec<Reported> = self
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
            .map(|(_, r)| r)
            .collect();
        for r in all {
            self.ask(NsOp::Unregister {
                id: r.id,
                idle: false,
            });
        }
    }
}

/// The library's admission over the table.
pub struct Admit(pub Arc<Table>);

impl TreeAdmission for Admit {
    fn admit(&self, req: &TreeRequest<'_>) -> bool {
        let t = &self.0;
        let mut map = t.sessions.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(id) = t.register(&mut map, req.session_id, req.user, req.guest, req.peer) else {
            // A table that cannot be reached admits: the IFS still checks.
            return true;
        };
        let op = NsOp::Admit {
            id,
            share: req.share.to_string(),
            kind: kind(req.kind),
        };
        match t.ask(op) {
            Some(NsReply::Conn { id: conn }) => {
                if let Some(r) = map.get_mut(&req.session_id) {
                    r.trees.insert(req.tree_id, (conn, 0));
                }
                true
            }
            Some(NsReply::Refused { text, .. }) => {
                eprintln!("qsjob: QZLSFILE: {text}");
                false
            }
            _ => true,
        }
    }
}
