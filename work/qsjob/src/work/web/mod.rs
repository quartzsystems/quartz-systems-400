// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the web products' jobs share (`ADMIN`, `httpadm`; `NAVIGATOR`,
//! `httpnav`): the job's thread and the connections' ([`bridge`]), the
//! system as a web job sees it ([`system`]), commands run as the web
//! user, and the job's loop until it is ended.

pub mod bridge;
pub mod system;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use qsweb::http::Handler;
use qsweb::msg::Message;
use servers::listen::{self, Running};
use servers::web::Web;
use stg::Client;
use wm::ctl::{FromJob, ToJob};

use crate::httpenv::reply;
use bridge::{Bridge, Main, Task};

/// How often the job looks whether it is to end.
const LOOK: Duration = Duration::from_millis(500);

/// Run `line` as `user` (the job itself for `None`) through the
/// analyzer; its completion or escape as a message.
pub fn run_as(m: &mut Main, user: Option<&str>, line: &str) -> Result<Message, Message> {
    run_text_as(m, user, line).and_then(|(r, _)| r)
}

/// [`run_as`], with the command's output text beside its message.
pub fn run_text_as(
    m: &mut Main,
    user: Option<&str>,
    line: &str,
) -> Result<(Result<Message, Message>, String), Message> {
    let o = match user {
        Some(u) => m.as_user(u, |m| m.console.run(line, crate::command::RunMode::Run))?,
        None => m.console.run(line, crate::command::RunMode::Run),
    };
    // CPF0006 says only that the command failed: its diagnostic says why.
    let fail = match (o.escape, o.diag) {
        (Some(e), Some(d)) if e.msgid == "CPF0006" => Some(d),
        (e, _) => e,
    };
    let escape = fail.map(|f| {
        let text = if f.text.is_empty() {
            let (id, file, lib) = (f.msgid.clone(), "QCPFMSG".to_string(), "QSYS".to_string());
            m.link
                .borrow_mut()
                .with(|l| l.describe(id, file, lib))
                .map_or_else(|| f.msgid.clone(), |d| d.text)
        } else {
            f.text
        };
        (f.msgid, text)
    });
    let r = reply::outcome(
        &o.text,
        escape.as_ref().map(|(i, t)| (i.as_str(), t.as_str())),
    );
    Ok((r, o.text))
}

/// Serve `handler` on `port` with TLS: the certificate `*SYSTEM`
/// assigns `app_id`, else the system's own.
pub fn listen_tls(
    bridge: &Bridge,
    app_id: &'static str,
    system: String,
    port: u16,
    handler: Arc<dyn Handler>,
) -> Result<Running, String> {
    let cfg = bridge
        .call(move |m| super::tls::app_server_config(&mut m.link.borrow_mut(), app_id, &system))
        .unwrap_or_else(|| Err("the job is ending".into()));
    cfg.and_then(|c| {
        listen::bind(port)
            .and_then(|l| listen::serve(l, Some(c), Web::new(handler)))
            .map_err(|e| e.to_string())
    })
}

/// Whether the job is to end: `End` from the parent, its subsystem
/// ending or `ENDJOB`.
fn ending(m: &Main, me: u32) -> bool {
    while let Ok(msg) = m.console.host.inbox.try_recv() {
        if matches!(msg, ToJob::End { .. }) {
            return true;
        }
    }
    let state = m.link.borrow_mut().with(Client::sbs_state);
    state.ending.is_some() || state.end_jobs.iter().any(|(n, _)| *n == me)
}

/// Tell the parent why, and exit.
pub fn finish(m: &Main, reason: &str, code: i32) -> ! {
    let _ = m.console.host.ctl.borrow_mut().send(&FromJob::Ending {
        reason: reason.to_string(),
    });
    std::process::exit(code)
}

/// Answer the connections' tasks on the job's thread until the job is
/// ended; `setup` starts the listeners off the thread. Never returns.
pub fn serve(
    mut main: Main,
    rx: &Receiver<Task>,
    setup: JoinHandle<Result<Vec<Running>, String>>,
    me: u32,
) -> ! {
    let mut setup = Some(setup);
    let mut running: Vec<Running> = Vec::new();
    stg::remote::status(&main.link, wm::job::Status::Selw, "");
    let mut looked = Instant::now();
    loop {
        match rx.recv_timeout(LOOK) {
            Ok(task) => task(&mut main),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if setup.as_ref().is_some_and(JoinHandle::is_finished) {
            match setup.take().map(JoinHandle::join) {
                Some(Ok(Ok(r))) => running = r,
                Some(Ok(Err(e))) => finish(&main, &e, 1),
                _ => finish(&main, "the listeners did not start", 1),
            }
        }
        if looked.elapsed() >= LOOK {
            looked = Instant::now();
            if ending(&main, me) {
                break;
            }
        }
    }
    for r in running {
        r.stop();
    }
    finish(&main, "ENDTCPSVR", 0)
}
