// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A netlink socket and the request patterns every family shares:
//! acknowledged requests, single replies and dumps.

#[cfg(not(target_os = "linux"))]
mod host;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(target_os = "linux"))]
use host as imp;
#[cfg(target_os = "linux")]
use linux as imp;

use std::time::Duration;

use crate::codec::{Builder, Msg, NLM_F_ACK, NLM_F_DUMP, NLMSG_DONE, NLMSG_ERROR, messages};
use crate::{Error, Result};

/// Requests give up after this long without a reply.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Receive buffer; dump datagrams stay well under it.
const RECV_BUF: usize = 256 * 1024;

/// A netlink protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    /// `NETLINK_ROUTE`.
    Route,
    /// `NETLINK_SOCK_DIAG`.
    SockDiag,
    /// `NETLINK_NETFILTER`.
    Netfilter,
}

/// A netlink socket bound to a kernel-assigned port.
pub struct Socket {
    raw: imp::Raw,
    seq: u32,
    buf: Vec<u8>,
}

impl Socket {
    /// A request socket on `bus`.
    pub fn open(bus: Bus) -> Result<Self> {
        let raw = imp::Raw::open(bus, 0)?;
        raw.set_timeout(Some(REQUEST_TIMEOUT))?;
        Ok(Self::with(raw))
    }

    /// A socket that receives the multicast `groups` (a bitmask of
    /// `RTMGRP_*`-style bits), with no receive timeout.
    pub fn subscribe(bus: Bus, groups: u32) -> Result<Self> {
        Ok(Self::with(imp::Raw::open(bus, groups)?))
    }

    fn with(raw: imp::Raw) -> Self {
        Self {
            raw,
            seq: 0,
            buf: vec![0; RECV_BUF],
        }
    }

    /// Set or clear the receive timeout.
    pub fn set_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.raw.set_timeout(timeout)?)
    }

    /// The next sequence number.
    pub fn next_seq(&mut self) -> u32 {
        self.seq = self.seq.wrapping_add(1);
        self.seq
    }

    /// Send finished bytes (one or more messages).
    pub fn send_bytes(&self, bytes: &[u8]) -> Result<()> {
        Ok(self.raw.send(bytes)?)
    }

    /// Receive one datagram.
    pub fn recv(&mut self) -> Result<&[u8]> {
        let n = self.raw.recv(&mut self.buf)?;
        Ok(&self.buf[..n])
    }

    /// Send `req` with `NLM_F_ACK` and wait for the acknowledgement.
    pub fn ack(&mut self, req: &Builder) -> Result<()> {
        let mut req = req.clone();
        req.add_flags(NLM_F_ACK);
        self.request(&req, |_| Ok(()))
    }

    /// Send `req` and hand every reply message to `f` until the request
    /// is complete: `NLMSG_DONE` for a dump, the acknowledgement for
    /// `NLM_F_ACK`, otherwise the first datagram with a reply.
    pub fn request(&mut self, req: &Builder, mut f: impl FnMut(&Msg<'_>) -> Result<()>) -> Result<()> {
        let seq = self.next_seq();
        let dump = req.flags() & NLM_F_DUMP == NLM_F_DUMP;
        let ack = req.flags() & NLM_F_ACK != 0;
        self.send_bytes(&req.finish(seq))?;
        loop {
            let mut replied = false;
            for m in messages(self.recv()?) {
                let m = m?;
                if m.seq != seq {
                    continue;
                }
                match m.kind {
                    NLMSG_ERROR => {
                        return match m.error() {
                            Some(0) => Ok(()),
                            Some(e) => Err(Error::Kernel(e)),
                            None => Err(Error::Malformed("error message")),
                        };
                    }
                    NLMSG_DONE => {
                        return match m.done_error() {
                            Some(0) | None => Ok(()),
                            Some(e) => Err(Error::Kernel(e)),
                        };
                    }
                    _ => {
                        f(&m)?;
                        replied = true;
                    }
                }
            }
            if replied && !dump && !ack {
                return Ok(());
            }
        }
    }

    /// Send a dump request and collect what `f` makes of each reply.
    pub fn dump<T>(
        &mut self,
        req: &Builder,
        mut f: impl FnMut(&Msg<'_>) -> Result<Option<T>>,
    ) -> Result<Vec<T>> {
        let mut out = Vec::new();
        self.request(req, |m| {
            if let Some(v) = f(m)? {
                out.push(v);
            }
            Ok(())
        })?;
        Ok(out)
    }
}
