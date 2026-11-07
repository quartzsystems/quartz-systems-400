// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request's panic answered as an error page. Release builds abort
//! on a panic, so it cannot unwind: each request runs on a thread of its
//! own, and the panic hook, on such a thread, logs the panic, hands the
//! connection the failure and parks that thread for good. The process
//! and its other connections go on. A lock the thread held stays held.

use std::cell::RefCell;
use std::panic::{self, PanicHookInfo};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Once};
use std::thread;

use super::{Handler, Peer, Request, Response};

/// Where a request's thread sends its answer.
type Answer = SyncSender<Result<Response, String>>;

thread_local! {
    /// Set on a request's thread while its handler runs.
    static REQUEST: RefCell<Option<Answer>> = const { RefCell::new(None) };
}

static HOOK: Once = Once::new();

/// Chain the hook that parks a failed request's thread; once per process.
pub fn install() {
    HOOK.call_once(|| {
        let prev = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let Some(tx) = REQUEST.with(|r| r.borrow_mut().take()) else {
                prev(info);
                return;
            };
            let text = describe(info);
            eprintln!("qsweb: a request failed: {text}");
            let _ = tx.send(Err(text));
            loop {
                thread::park();
            }
        }));
    });
}

/// The panic's message and place.
fn describe(info: &PanicHookInfo<'_>) -> String {
    let p = info.payload();
    let msg = p
        .downcast_ref::<&str>()
        .map(ToString::to_string)
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic".into());
    match info.location() {
        Some(l) => format!("{msg} at {}:{}", l.file(), l.line()),
        None => msg,
    }
}

/// `handler`'s response to `req`, or why it has none.
pub fn handle(handler: &Arc<dyn Handler>, req: Request, peer: Peer) -> Result<Response, String> {
    install();
    let (tx, rx) = mpsc::sync_channel(1);
    let (h, done) = (Arc::clone(handler), tx.clone());
    thread::Builder::new()
        .name("http-request".into())
        .spawn(move || {
            REQUEST.with(|r| *r.borrow_mut() = Some(tx));
            let resp = h.handle(&req, &peer);
            REQUEST.with(|r| r.borrow_mut().take());
            let _ = done.send(Ok(resp));
        })
        .map_err(|e| format!("no thread for the request: {e}"))?;
    rx.recv()
        .unwrap_or_else(|_| Err("the request ended without an answer".into()))
}

/// The page a failed request gets.
pub fn failed() -> Response {
    Response::html(
        500,
        "<!DOCTYPE html><html><head><title>Error</title></head><body><h1>The page could not be shown</h1>\
         <p>An internal error stopped this page. The server is still running: go back and try another page. \
         The error is logged on the system console.</p></body></html>"
            .to_string(),
    )
}
