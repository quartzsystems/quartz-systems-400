// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QTLPD00001` in `QSYSWRK`, as `QTMPLPD`: the line printer daemon
//! (`STRTCPSVR *LPD`). Each connection is served by `net/lpd` on its
//! own thread; what needs the store (is the queue an output queue,
//! spool this job, what is on the queue) comes to this thread, which
//! holds the job's link. A queue name is an output queue, `LIB/NAME`
//! or a name found through the library list or in `QUSRSYS`. The
//! spooled files belong to this job and so to `QTMPLPD`.

use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ipc::chan::Chan;
use lpd::session::{Job, Limits, Spooler};
use servers::attrs::lpd::LpdAttrs;
use servers::listen::{self, Conn, Service};
use servers::msg;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::obj::splf::SplfAttrs;

use super::netsvc::{self, Ender};

const PGM: &str = "QTMPLPD";

/// A connection's question for the job's thread.
enum Ask {
    /// The output queue a queue name means.
    Outq(String, Sender<Option<String>>),
    /// Spool a job from a peer.
    Submit(Job, String, Sender<Result<(), String>>),
    /// The queue state.
    State(String, bool, Vec<String>, Sender<String>),
}

/// One connection's spooler: questions to the job's thread.
struct Bridge {
    tx: Sender<Ask>,
    peer: String,
}

impl Bridge {
    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Ask, dft: T) -> T {
        let (tx, rx) = mpsc::channel();
        if self.tx.send(make(tx)).is_err() {
            return dft;
        }
        rx.recv_timeout(Duration::from_secs(60)).unwrap_or(dft)
    }
}

impl Spooler for Bridge {
    fn accepts(&mut self, queue: &str) -> bool {
        let q = queue.to_string();
        self.ask(|tx| Ask::Outq(q, tx), None).is_some()
    }

    fn submit(&mut self, job: Job) -> Result<(), String> {
        let peer = self.peer.clone();
        self.ask(|tx| Ask::Submit(job, peer, tx), Err("the LPD job is ending".into()))
    }

    fn state(&mut self, queue: &str, long: bool, list: &[String]) -> String {
        let (q, l) = (queue.to_string(), list.to_vec());
        self.ask(|tx| Ask::State(q, long, l, tx), String::new())
    }

    fn remove(&mut self, queue: &str, _agent: &str, _list: &[String]) -> String {
        format!("{queue}: jobs are removed with DLTSPLF on this system\n")
    }
}

/// The accept loop's service.
struct Daemon {
    tx: Mutex<Sender<Ask>>,
}

impl Service for Daemon {
    fn serve(&self, mut conn: Conn, peer: SocketAddr) {
        let tx = self.tx.lock().map(|t| t.clone());
        let Ok(tx) = tx else { return };
        let _ = conn.tcp().set_read_timeout(Some(Duration::from_secs(120)));
        let mut sp = Bridge {
            tx,
            peer: peer.ip().to_string(),
        };
        if let Err(e) = lpd::serve(&mut conn, &mut sp, Limits::default()) {
            eprintln!("qsjob: {PGM}: {peer}: {e}");
        }
        conn.close();
    }
}

/// The output queue `queue` names, `LIB/NAME`.
fn outq(link: &Shared, queue: &str) -> Option<String> {
    let q = queue.trim().to_ascii_uppercase();
    let (lib, name) = match q.split_once('/') {
        Some((l, n)) => (l.to_string(), n.to_string()),
        None => ("*LIBL".to_string(), q.clone()),
    };
    let ok = |n: &str| {
        (1..=10).contains(&n.len())
            && !n.starts_with(|c: char| c.is_ascii_digit())
            && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '@' | '$' | '_' | '*'))
    };
    if !ok(&name) || !ok(&lib) {
        return None;
    }
    let mut libs = vec![lib.clone()];
    if lib == "*LIBL" {
        libs.push("QUSRSYS".into());
    }
    libs.into_iter().find_map(|l| {
        link.borrow_mut()
            .with(|c| c.obj_sizes(l, name.clone(), "*OUTQ".into()))
            .into_iter()
            .next()
            .map(|(lib, name, _, _)| format!("{}/{}", lib.trim(), name.trim()))
    })
}

/// Spool every file of `job` onto its queue.
fn submit(link: &Shared, job: &Job, peer: &str) -> Result<(), String> {
    let q = outq(link, &job.queue).ok_or_else(|| format!("{} is not an output queue", job.queue))?;
    for s in lpd::convert::spooled(job) {
        let a = &s.attrs;
        let attrs = SplfAttrs {
            name: a.file.clone(),
            usrdta: a.usrdta.chars().take(10).collect(),
            copies: a.copies.max(1),
            outpty: a.outpty,
            save: a.save,
            page_len: a.page_len,
            page_width: a.page_width,
            lpi: a.lpi,
            cpi: a.cpi,
            ovrflw: a.ovrflw,
            drawer: a.drawer,
            duplex: match a.duplex {
                print::job::Duplex::No => 0,
                print::job::Duplex::Yes => 1,
                print::job::Duplex::Tumble => 2,
            },
            formtype: a.formtype.clone(),
            outq: q.clone(),
            devtype: a.devtype.clone(),
            font: a.font.clone(),
            prtf: a.prtf.clone(),
            usrdfndta: a.usrdfndta.clone(),
            ..SplfAttrs::default()
        };
        let id = link.borrow_mut().with(|l| l.splf_open(attrs))?;
        link.borrow_mut().with(|l| l.splf_put(id, s.data.clone(), s.pages, s.records))?;
        link.borrow_mut().with(|l| l.splf_close(id, false))?;
        let from = if s.user.is_empty() { peer.to_string() } else { format!("{}@{peer}", s.user) };
        netsvc::joblog(
            link,
            PGM,
            msg::LPD_SPOOLED,
            &format!("LPD spooled file {} from {from} into {q}.", a.file),
        );
    }
    Ok(())
}

/// The queue state text.
fn state(link: &Shared, queue: &str, long: bool, list: &[String]) -> String {
    let Some(q) = outq(link, queue) else {
        return format!("{queue}: not an output queue on this system\n");
    };
    let entries: Vec<lpd::status::Entry> = link
        .borrow_mut()
        .with(|l| l.splf_list(String::new(), String::new()))
        .into_iter()
        .filter(|a| a.outq.eq_ignore_ascii_case(&q))
        .map(|a| lpd::status::Entry {
            owner: a.job.1.clone(),
            job: a.job.2,
            file: a.name.clone(),
            number: a.number,
            status: a.status.code().to_string(),
            size: a.data_len,
        })
        .collect();
    lpd::status::text(&q, &entries, long, list)
}

fn answer(link: &Shared, ask: Ask) {
    match ask {
        Ask::Outq(q, tx) => {
            let _ = tx.send(outq(link, &q));
        }
        Ask::Submit(job, peer, tx) => {
            let r = submit(link, &job, &peer);
            if let Err(e) = &r {
                netsvc::joblog(link, PGM, msg::LPD_REFUSED, &format!("LPD job from {peer} not spooled: {e}."));
            }
            let _ = tx.send(r);
        }
        Ask::State(q, long, list, tx) => {
            let _ = tx.send(state(link, &q, long, &list));
        }
    }
}

/// Run the daemon. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let attrs = LpdAttrs::from_bag(&netsvc::bag(link, "*LPD"));
    let (tx, rx): (Sender<Ask>, Receiver<Ask>) = mpsc::channel();
    let daemon = Arc::new(Daemon { tx: Mutex::new(tx) });
    let running = match listen::bind(attrs.port).and_then(|l| listen::serve(l, None, daemon)) {
        Ok(r) => r,
        Err(e) => crate::start::end(&mut ctl, &format!("port {}: {e}", attrs.port)),
    };
    eprintln!("qsjob: {} listening on port {}", start.name, attrs.port);
    let ender = Ender::new(&ctl, start.number);
    stg::remote::status(link, wm::job::Status::Selw, "");
    loop {
        // Questions first; the end is looked for between them.
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(ask) => {
                answer(link, ask);
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if ender.ended(link, Duration::from_millis(0)) {
            break;
        }
    }
    running.stop();
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDTCPSVR".into(),
    });
    std::process::exit(0)
}
