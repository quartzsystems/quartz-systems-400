// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer administration from a job (Navigator's `NAVIGATOR` job,
//! QAP's connection jobs): the `QZLS*` APIs through the storage
//! service (`netsvr_api`) as the job's current profile, so the service
//! checks that profile's authority; the session table (`netsvr` `List`)
//! and the users disabled for NetServer (`netsvr_disabled_users`).

#![allow(clippy::doc_markdown)] // NetServer

use netsvrcfg::client::Failure;
use netsvrcfg::{Connection, Session};
use stg::netsvr::{NsOp, NsReply, NsSession};
use stg::{Client, JobInfo};

/// The service's answer to an API call, as the client takes it.
pub fn call(link: &mut stg::Link, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure> {
    link.with(|l| l.netsvr_api(api.to_string(), parms))
        .map_err(|(id, data)| Failure { id, data })
}

fn session(s: NsSession) -> Session {
    Session {
        id: s.id,
        workstation: s.workstation,
        expanded: s.expanded,
        user: s.user,
        job: s.job,
        guest: s.guest,
        since: s.since,
        last_active: s.last_active,
        ending: s.ending,
        connections: s
            .connections
            .into_iter()
            .map(|c| Connection {
                id: c.id,
                share: c.share,
                kind: c.kind,
                opens: c.opens,
                since: c.since,
            })
            .collect(),
    }
}

/// Every session, as the service's table holds it.
pub fn sessions(link: &mut stg::Link) -> Vec<Session> {
    match link.with(|l| l.netsvr(NsOp::List)) {
        NsReply::Sessions { list } => list.into_iter().map(session).collect(),
        _ => Vec::new(),
    }
}

/// The users disabled for NetServer and since when (seconds since
/// 1970), by name.
pub fn disabled(link: &mut stg::Link) -> Vec<(String, u64)> {
    let mut v = link.with(Client::netsvr_disabled_users);
    v.sort();
    v
}

/// `QZLSSERVER`'s job (`number/user/name`) when it is active in
/// `QSERVER`, as the APIs count it.
pub fn server_job(jobs: &[JobInfo]) -> Option<String> {
    jobs.iter()
        .find(|j| j.name.trim() == "QZLSSERVER" && j.subsystem.trim() == "QSERVER")
        .map(|j| format!("{:06}/{}/{}", j.number, j.user.trim(), j.name.trim()))
}

/// [`server_job`] over the active job table.
pub fn server(link: &mut stg::Link) -> Option<String> {
    let jobs = link.with(Client::active_jobs);
    server_job(&jobs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_cross_over() {
        let s = session(NsSession {
            id: 4,
            workstation: "PC1".into(),
            connections: vec![stg::netsvr::NsConn {
                id: 2,
                share: "QIBM".into(),
                kind: 0,
                opens: 3,
                since: 9,
            }],
            ..NsSession::default()
        });
        assert_eq!((s.id, s.opens()), (4, 3));
    }
}
