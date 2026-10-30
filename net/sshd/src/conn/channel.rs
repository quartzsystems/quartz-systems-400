// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection protocol (RFC 4254): channels, their windows, session
//! requests, forwarding and global requests.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};

use super::credit::{ChannelWriter, Credit};
use super::{Conn, Duplex, Event, Forward, MAX_PACKET, WINDOW};
use crate::config::pattern;
use crate::error::SshResult;
use crate::io::pipe::{PipeWriter, pipe};
use crate::session::{ChannelIo, Control, Exit, Pty, SessionKind, SessionRequest};
use crate::wire::{Reader, Writer, msg};

/// One open channel.
pub(super) struct Channel {
    remote: u32,
    credit: Arc<Credit>,
    max_packet: usize,
    consumed: u32,
    session: bool,
    stdin: Option<PipeWriter>,
    control: Option<Sender<Control>>,
    pty: Option<Pty>,
    env: Vec<(String, String)>,
    started: bool,
    sent_eof: bool,
    sent_close: bool,
}

impl Channel {
    fn new(remote: u32, window: u32, max_packet: u32, session: bool) -> Self {
        Self {
            remote,
            credit: Credit::new(window),
            max_packet: (max_packet as usize).clamp(1024, 256 * 1024),
            consumed: 0,
            session,
            stdin: None,
            control: None,
            pty: None,
            env: Vec::new(),
            started: false,
            sent_eof: false,
            sent_close: false,
        }
    }

    /// The connection ended: unblock whatever still runs on it.
    pub(super) fn abandon(self) {
        self.credit.close();
        if let Some(s) = &self.stdin {
            s.close();
        }
    }
}

const PROHIBITED: &str = "Port forwarding is not allowed.";

impl<W: Write> Conn<'_, W> {
    fn new_chan_id(&mut self) -> u32 {
        let id = self.next_chan;
        self.next_chan = self.next_chan.wrapping_add(1);
        id
    }

    fn writer(&self, chan: u32, ch: &Channel, ext: bool) -> ChannelWriter {
        ChannelWriter {
            tx: self.tx.clone(),
            credit: ch.credit.clone(),
            chan,
            ext,
            max_packet: ch.max_packet,
        }
    }

    fn stdin_pipe(&self, chan: u32) -> (PipeWriter, crate::io::pipe::PipeReader) {
        let tx = self.tx.clone();
        pipe(Some(Box::new(move |n| {
            let _ = tx.send(Event::Consumed { chan, n });
        })))
    }

    fn open_failure_msg(remote: u32, code: u32, text: &str) -> Vec<u8> {
        let mut w = Writer::msg(msg::CHANNEL_OPEN_FAILURE);
        w.u32(remote).u32(code).str(text).str("");
        w.finish()
    }

    fn confirm(remote: u32, local: u32) -> Vec<u8> {
        let mut w = Writer::msg(msg::CHANNEL_OPEN_CONFIRMATION);
        w.u32(remote).u32(local).u32(WINDOW).u32(MAX_PACKET);
        w.finish()
    }

    pub(super) fn channel_open(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let kind = r.str()?.to_string();
        let remote = r.u32()?;
        let window = r.u32()?;
        let max = r.u32()?;
        match kind.as_str() {
            "session" => {
                let sessions = self.channels.values().filter(|c| c.session).count();
                let limit = self.srv.config.max_sessions as usize;
                if self.no_more_sessions || sessions >= limit.max(1) {
                    return self.send(Self::open_failure_msg(
                        remote,
                        msg::open_failure::PROHIBITED,
                        "No more sessions are allowed on this connection.",
                    ));
                }
                let id = self.new_chan_id();
                self.channels.insert(id, Channel::new(remote, window, max, true));
                self.send(Self::confirm(remote, id))
            }
            "direct-tcpip" => {
                let host = r.str()?.to_string();
                let port = r.u32()?;
                if !self.local_forward_allowed() {
                    return self.send(Self::open_failure_msg(remote, msg::open_failure::PROHIBITED, PROHIBITED));
                }
                let conn = u16::try_from(port)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
                    .and_then(|p| self.srv.deps.forwarder.connect(&host, p));
                match conn {
                    Ok(d) => {
                        let id = self.new_chan_id();
                        let mut ch = Channel::new(remote, window, max, false);
                        self.start_pumps(id, &mut ch, d);
                        self.channels.insert(id, ch);
                        self.send(Self::confirm(remote, id))
                    }
                    Err(e) => self.send(Self::open_failure_msg(
                        remote,
                        msg::open_failure::CONNECT_FAILED,
                        &format!("Connecting to {host} port {port} failed: {e}"),
                    )),
                }
            }
            _ => self.send(Self::open_failure_msg(
                remote,
                msg::open_failure::UNKNOWN_CHANNEL_TYPE,
                "The channel type is not supported.",
            )),
        }
    }

    fn local_forward_allowed(&self) -> bool {
        self.srv.config.allow_tcp_forwarding.local()
            && self.authed.as_ref().is_some_and(|a| !a.options.no_port_forwarding)
    }

    /// Wire a forwarded connection to channel `id`.
    fn start_pumps(&self, id: u32, ch: &mut Channel, d: Duplex) {
        let (stdin_w, mut stdin_r) = self.stdin_pipe(id);
        let mut out = self.writer(id, ch, false);
        let Duplex {
            read: mut from,
            write: mut to,
            ..
        } = d;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut from, &mut out);
            let _ = tx.send(Event::Finished { chan: id, exit: None });
        });
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut stdin_r, &mut to);
        });
        ch.stdin = Some(stdin_w);
        ch.started = true;
    }

    pub(super) fn incoming(&mut self, bind: &(String, u16), conn: Duplex) -> SshResult<()> {
        let id = self.new_chan_id();
        let (oaddr, oport) = conn.peer.clone();
        self.pending_opens.insert(id, conn);
        let mut w = Writer::msg(msg::CHANNEL_OPEN);
        w.str("forwarded-tcpip")
            .u32(id)
            .u32(WINDOW)
            .u32(MAX_PACKET)
            .str(&bind.0)
            .u32(u32::from(bind.1))
            .str(&oaddr)
            .u32(u32::from(oport));
        self.send(w.finish())
    }

    pub(super) fn open_confirmation(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        let remote = r.u32()?;
        let window = r.u32()?;
        let max = r.u32()?;
        let Some(d) = self.pending_opens.remove(&local) else {
            return Ok(());
        };
        let mut ch = Channel::new(remote, window, max, false);
        self.start_pumps(local, &mut ch, d);
        self.channels.insert(local, ch);
        Ok(())
    }

    pub(super) fn open_failure(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        self.pending_opens.remove(&local);
        Ok(())
    }

    pub(super) fn window_adjust(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        let n = r.u32()?;
        if let Some(ch) = self.channels.get(&local) {
            ch.credit.add(n);
        }
        Ok(())
    }

    pub(super) fn channel_data_in(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        let kind = r.u8()?;
        let local = r.u32()?;
        if kind == msg::CHANNEL_EXTENDED_DATA {
            r.u32()?;
        }
        let data = r.bytes()?;
        if let Some(ch) = self.channels.get_mut(&local)
            && let Some(w) = ch.stdin.as_mut()
        {
            if w.write_all(data).is_err() {
                ch.stdin = None;
            }
        } else if let Some(ch) = self.channels.get_mut(&local) {
            // Nothing reads it: hand the window back at once.
            ch.consumed += u32::try_from(data.len()).unwrap_or(0);
            return self.consumed(local, 0);
        }
        Ok(())
    }

    pub(super) fn consumed(&mut self, chan: u32, n: usize) -> SshResult<()> {
        let Some(ch) = self.channels.get_mut(&chan) else {
            return Ok(());
        };
        ch.consumed = ch.consumed.saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
        if ch.consumed >= WINDOW / 2 && !ch.sent_close {
            let mut w = Writer::msg(msg::CHANNEL_WINDOW_ADJUST);
            w.u32(ch.remote).u32(ch.consumed);
            ch.consumed = 0;
            return self.send(w.finish());
        }
        Ok(())
    }

    pub(super) fn channel_data_out(&mut self, chan: u32, ext: bool, data: &[u8]) -> SshResult<()> {
        let Some(ch) = self.channels.get(&chan) else {
            return Ok(());
        };
        if ch.sent_eof || ch.sent_close {
            return Ok(());
        }
        let mut w = if ext {
            let mut w = Writer::msg(msg::CHANNEL_EXTENDED_DATA);
            w.u32(ch.remote).u32(msg::STDERR);
            w
        } else {
            let mut w = Writer::msg(msg::CHANNEL_DATA);
            w.u32(ch.remote);
            w
        };
        w.bytes(data);
        self.send(w.finish())
    }

    pub(super) fn channel_eof(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        if let Some(ch) = self.channels.get_mut(&local) {
            ch.stdin = None;
        }
        Ok(())
    }

    pub(super) fn channel_close(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        let Some(ch) = self.channels.remove(&local) else {
            return Ok(());
        };
        let (remote, sent) = (ch.remote, ch.sent_close);
        ch.abandon();
        if !sent {
            let mut w = Writer::msg(msg::CHANNEL_CLOSE);
            w.u32(remote);
            self.send(w.finish())?;
        }
        Ok(())
    }

    pub(super) fn finished(&mut self, chan: u32, exit: Option<Exit>) -> SshResult<()> {
        let Some(ch) = self.channels.get_mut(&chan) else {
            return Ok(());
        };
        let remote = ch.remote;
        let (eof, close) = (!ch.sent_eof, !ch.sent_close);
        ch.sent_eof = true;
        ch.sent_close = true;
        match exit {
            Some(Exit::Status(n)) => {
                let mut w = Writer::msg(msg::CHANNEL_REQUEST);
                w.u32(remote).str("exit-status").bool(false).u32(n);
                self.send(w.finish())?;
            }
            Some(Exit::Signal(name)) => {
                let mut w = Writer::msg(msg::CHANNEL_REQUEST);
                w.u32(remote)
                    .str("exit-signal")
                    .bool(false)
                    .str(&name)
                    .bool(false)
                    .str("")
                    .str("");
                self.send(w.finish())?;
            }
            None => {}
        }
        if eof {
            let mut w = Writer::msg(msg::CHANNEL_EOF);
            w.u32(remote);
            self.send(w.finish())?;
        }
        if close {
            let mut w = Writer::msg(msg::CHANNEL_CLOSE);
            w.u32(remote);
            self.send(w.finish())?;
        }
        Ok(())
    }

    fn reply(&mut self, remote: u32, want: bool, ok: bool) -> SshResult<()> {
        if !want {
            return Ok(());
        }
        let mut w = Writer::msg(if ok {
            msg::CHANNEL_SUCCESS
        } else {
            msg::CHANNEL_FAILURE
        });
        w.u32(remote);
        self.send(w.finish())
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn channel_request(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let local = r.u32()?;
        let kind = r.str()?.to_string();
        let want = r.bool()?;
        let Some(ch) = self.channels.get_mut(&local) else {
            return Ok(());
        };
        let remote = ch.remote;
        if !ch.session {
            return self.reply(remote, want, false);
        }
        let no_pty = self.authed.as_ref().is_some_and(|a| a.options.no_pty);
        let ok = match kind.as_str() {
            "pty-req" if !ch.started && !no_pty => {
                let term = r.str()?.to_string();
                let cols = r.u32()?;
                let rows = r.u32()?;
                r.u32()?;
                r.u32()?;
                let modes = r.bytes()?.to_vec();
                ch.pty = Some(Pty {
                    term,
                    cols,
                    rows,
                    modes,
                });
                true
            }
            "env" if !ch.started => {
                let name = r.str()?.to_string();
                let value = r.str()?.to_string();
                let accepted = self
                    .srv
                    .config
                    .accept_env
                    .iter()
                    .any(|p| pattern::wildcard(&name, p));
                if accepted {
                    ch.env.push((name, value));
                }
                accepted
            }
            "window-change" => {
                let cols = r.u32()?;
                let rows = r.u32()?;
                if let Some(c) = &ch.control {
                    let _ = c.send(Control::WindowChange { cols, rows });
                }
                return Ok(());
            }
            "signal" => {
                let name = r.str()?.to_string();
                if let Some(c) = &ch.control {
                    let _ = c.send(Control::Signal(name));
                }
                return Ok(());
            }
            "shell" | "exec" | "subsystem" if !ch.started => {
                let kind = match kind.as_str() {
                    "shell" => Some(SessionKind::Shell),
                    "exec" => Some(SessionKind::Exec(r.str()?.to_string())),
                    _ => {
                        let name = r.str()?.to_string();
                        self.srv
                            .config
                            .subsystem(&name)
                            .map(|command| SessionKind::Subsystem {
                                name: name.clone(),
                                command: command.to_string(),
                            })
                    }
                };
                match kind {
                    Some(k) => {
                        self.start_session(local, k);
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        };
        self.reply(remote, want, ok)
    }

    fn start_session(&mut self, chan: u32, kind: SessionKind) {
        let Some(authed) = self.authed.clone() else {
            return;
        };
        let (kind, original) = match &authed.options.command {
            Some(forced) => {
                let orig = match &kind {
                    SessionKind::Exec(c) => Some(c.clone()),
                    _ => None,
                };
                (SessionKind::Exec(forced.clone()), orig)
            }
            None => (kind, None),
        };
        let (stdin_w, stdin_r) = self.stdin_pipe(chan);
        let (ctl_tx, ctl_rx) = mpsc::channel();
        let Some(ch) = self.channels.get(&chan) else {
            return;
        };
        let io = ChannelIo {
            stdin: Box::new(stdin_r),
            stdout: Box::new(self.writer(chan, ch, false)),
            stderr: Box::new(self.writer(chan, ch, true)),
            control: ctl_rx,
        };
        let req = SessionRequest {
            account: authed.account,
            kind,
            pty: ch.pty.clone(),
            env: ch.env.clone(),
            peer: self.peer.clone(),
            local: self.local.clone(),
            original_command: original,
        };
        if let Some(ch) = self.channels.get_mut(&chan) {
            ch.started = true;
            ch.stdin = Some(stdin_w);
            ch.control = Some(ctl_tx);
        }
        let host = self.srv.deps.host.clone();
        let tx = self.tx.clone();
        tracing::debug!(user = req.account.name, kind = ?req.kind, "session started");
        std::thread::spawn(move || {
            let exit = host.run(req, io);
            let _ = tx.send(Event::Finished {
                chan,
                exit: Some(exit),
            });
        });
    }

    pub(super) fn global_request(&mut self, m: &[u8]) -> SshResult<()> {
        let mut r = Reader::new(m);
        r.u8()?;
        let name = r.str()?.to_string();
        let want = r.bool()?;
        let answer: Option<Vec<u8>> = match name.as_str() {
            "tcpip-forward" => {
                let addr = r.str()?.to_string();
                let port = r.u32()?;
                self.remote_forward(&addr, port)
            }
            "cancel-tcpip-forward" => {
                let addr = r.str()?.to_string();
                let port = u16::try_from(r.u32()?).unwrap_or(0);
                self.forwards.remove(&(addr, port)).map(|f| {
                    f.stop.store(true, std::sync::atomic::Ordering::SeqCst);
                    Vec::new()
                })
            }
            "no-more-sessions@openssh.com" => {
                self.no_more_sessions = true;
                return Ok(());
            }
            _ => None,
        };
        if !want {
            return Ok(());
        }
        match answer {
            Some(extra) => {
                let mut w = Writer::msg(msg::REQUEST_SUCCESS);
                w.raw(&extra);
                self.send(w.finish())
            }
            None => self.send(vec![msg::REQUEST_FAILURE]),
        }
    }

    /// `tcpip-forward`: the success payload, or `None` when refused.
    fn remote_forward(&mut self, addr: &str, port: u32) -> Option<Vec<u8>> {
        let authed = self.authed.as_ref()?;
        if !self.srv.config.allow_tcp_forwarding.remote() || authed.options.no_port_forwarding {
            tracing::info!(user = authed.account.name, port, "remote forward refused by the configuration");
            return None;
        }
        let port = u16::try_from(port).ok()?;
        // IPPROTO_TCP.
        if !self.srv.deps.ports.may_bind(6, port, &authed.account.name) {
            tracing::info!(user = authed.account.name, port, "remote forward refused by a port restriction");
            return None;
        }
        let listener = self.srv.deps.forwarder.listen(addr, port).ok()?;
        let bound = listener.port();
        let stop = Arc::new(AtomicBool::new(false));
        let key = (addr.to_string(), if port == 0 { bound } else { port });
        let tx = self.tx.clone();
        let stop2 = stop.clone();
        let bind = (addr.to_string(), bound);
        std::thread::spawn(move || {
            while let Some(conn) = listener.accept(&stop2) {
                if tx
                    .send(Event::Incoming {
                        bind: bind.clone(),
                        conn,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.forwards.insert(key, Forward { stop });
        let mut w = Writer::new();
        if port == 0 {
            w.u32(u32::from(bound));
        }
        Some(w.finish())
    }
}
