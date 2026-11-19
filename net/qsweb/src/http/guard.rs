// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request's panic answered as an error page. Release builds abort
//! on a panic, so it cannot unwind: each request runs on a thread of its
//! own, and the panic hook, on such a thread, logs the panic, hands the
//! connection the failure and parks that thread for good. The process
//! and its other connections go on. A lock the thread held stays held.

use std::cell::RefCell;
use std::io::{self, Read};
use std::panic::{self, PanicHookInfo};
use std::sync::mpsc::{self, Receiver, SyncSender};
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
            BODY.with(|b| b.borrow_mut().take());
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

/// One piece of a streamed body, or why the connection failed.
type Piece = Result<Vec<u8>, String>;

thread_local! {
    /// Set on a streamed request's thread: where its body arrives. The
    /// panic hook drops it, so the connection stops sending.
    static BODY: RefCell<Option<Receiver<Piece>>> = const { RefCell::new(None) };
}

/// The streamed body as the handler's thread reads it.
struct Piped {
    have: Vec<u8>,
    at: usize,
}

impl Read for Piped {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.have.len() {
            let next = BODY.with(|b| b.borrow().as_ref().map(Receiver::recv));
            match next {
                Some(Ok(Ok(piece))) => {
                    self.have = piece;
                    self.at = 0;
                }
                Some(Ok(Err(e))) => return Err(io::Error::other(e)),
                // The sender is done: the body ended.
                Some(Err(_)) | None => return Ok(0),
            }
        }
        let n = buf.len().min(self.have.len() - self.at);
        buf[..n].copy_from_slice(&self.have[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// [`handle`] for a request whose body streams: the handler runs on
/// its own thread and reads `body` through a channel this thread
/// fills; what it leaves unread stays in `body`.
pub fn handle_stream(
    handler: &Arc<dyn Handler>,
    req: Request,
    peer: Peer,
    body: &mut dyn Read,
) -> Result<Response, String> {
    install();
    let (tx, rx) = mpsc::sync_channel(1);
    let (pieces, arrive) = mpsc::sync_channel::<Piece>(4);
    let (h, done) = (Arc::clone(handler), tx.clone());
    thread::Builder::new()
        .name("http-request".into())
        .spawn(move || {
            REQUEST.with(|r| *r.borrow_mut() = Some(tx));
            BODY.with(|b| *b.borrow_mut() = Some(arrive));
            let resp = h.handle_stream(
                &req,
                &mut Piped {
                    have: Vec::new(),
                    at: 0,
                },
                &peer,
            );
            BODY.with(|b| b.borrow_mut().take());
            REQUEST.with(|r| r.borrow_mut().take());
            let _ = done.send(Ok(resp));
        })
        .map_err(|e| format!("no thread for the request: {e}"))?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let piece = match body.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => Ok(buf[..n].to_vec()),
            Err(e) => Err(e.to_string()),
        };
        let failed = piece.is_err();
        // Refused: the handler is done with the body, or failed.
        if pieces.send(piece).is_err() || failed {
            break;
        }
    }
    drop(pieces);
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
