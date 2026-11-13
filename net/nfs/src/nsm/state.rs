// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The status monitor's table: its state number (odd while it runs)
//! and the hosts local programs asked it to monitor. Each change is
//! written through the [`StatdStore`]. A start ([`NsmState::start`])
//! moves the state up by two (to odd from a missing or even number)
//! and hands back the hosts monitored before, to be told
//! ([`NsmState::notify_hosts`]); a host's `SM_NOTIFY` calls back every
//! local program monitoring it ([`NsmState::notified`]).

use std::io;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use super::record::{
    MonRecord, StatdStore, decode_monitored, decode_state, encode_monitored, encode_state,
};
use crate::rpc::{ClientConfig, Credential, Proto, RpcClient};
use crate::server::locate::{Host, Locator};
use crate::types::nsm::{SM_PROG, status};
use crate::xdr;

/// How the monitor reaches others.
#[derive(Clone)]
pub struct NsmConfig {
    /// This host's name in `SM_NOTIFY`.
    pub me: String,
    /// Where programs listen.
    pub locator: Arc<dyn Locator>,
    /// How each call is made.
    pub rpc: ClientConfig,
    /// Rounds of `SM_NOTIFY` to a host that does not answer.
    pub notify_rounds: u32,
    /// The pause between rounds.
    pub notify_pause: Duration,
}

impl std::fmt::Debug for NsmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NsmConfig")
            .field("me", &self.me)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
struct Inner {
    state: u32,
    mon: Vec<MonRecord>,
}

/// Calls the monitor makes after it answered.
#[derive(Debug, Clone)]
pub enum Work {
    /// A host (name, address) restarted in a state: call back its monitors.
    Notified(Vec<u8>, IpAddr, i32),
    /// Tell these hosts this one restarted.
    Tell(Vec<MonRecord>),
}

/// How many pieces of [`Work`] wait at most; past it they are dropped.
pub const WORK_QUEUE: usize = 256;

/// The monitor.
pub struct NsmState {
    cfg: NsmConfig,
    store: Box<dyn StatdStore>,
    inner: Mutex<Inner>,
    work: Mutex<Option<mpsc::SyncSender<Work>>>,
}

impl std::fmt::Debug for NsmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NsmState")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

/// The next state after `old` for a start: odd, above it.
#[must_use]
pub fn next_state(old: Option<u32>) -> u32 {
    match old {
        None | Some(0) => 1,
        Some(s) if s % 2 == 1 => s.wrapping_add(2).max(1),
        Some(s) => s.wrapping_add(1),
    }
}

fn same_host(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

impl NsmState {
    /// A start: the state moved up, the monitored hosts of before
    /// handed back (and forgotten: they monitor again as they lock).
    ///
    /// # Errors
    ///
    /// The store's.
    pub fn start(
        store: Box<dyn StatdStore>,
        cfg: NsmConfig,
    ) -> io::Result<(Arc<Self>, Vec<MonRecord>)> {
        let old = store.read_state()?.as_deref().and_then(decode_state);
        let state = next_state(old);
        store.write_state(&encode_state(state))?;
        let before = match store.read_monitored()? {
            Some(b) => decode_monitored(&b).unwrap_or_default(),
            None => Vec::new(),
        };
        store.write_monitored(&encode_monitored(&[]))?;
        let s = Arc::new(Self {
            cfg,
            store,
            inner: Mutex::new(Inner {
                state,
                mon: Vec::new(),
            }),
            work: Mutex::new(None),
        });
        Ok((s, before))
    }

    /// From now on [`Work`] goes to a worker: what it reads.
    pub fn worker(&self) -> mpsc::Receiver<Work> {
        let (tx, rx) = mpsc::sync_channel(WORK_QUEUE);
        *self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx);
        rx
    }

    /// Does `w` on the worker, or now when there is none.
    pub fn later(&self, w: Work) {
        let tx = self
            .work
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        match tx {
            Some(tx) => {
                if tx.try_send(w).is_err() {
                    tracing::warn!("status monitor work dropped: the queue is full");
                }
            }
            None => self.run(w, &AtomicBool::new(false)),
        }
    }

    /// Does `w`.
    pub fn run(&self, w: Work, stop: &AtomicBool) {
        match w {
            Work::Notified(name, peer, state) => {
                self.notified(&name, peer, state);
            }
            Work::Tell(hosts) => {
                self.notify_hosts(&hosts, stop);
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn save(&self, g: &Inner) {
        if let Err(e) = self.store.write_monitored(&encode_monitored(&g.mon)) {
            tracing::warn!("monitored hosts not written: {e}");
        }
    }

    /// The state number.
    pub fn state(&self) -> u32 {
        self.lock().state
    }

    /// The state as the protocol's `int`.
    pub fn state_i32(&self) -> i32 {
        i32::from_ne_bytes(self.state().to_ne_bytes())
    }

    /// The hosts monitored now.
    pub fn monitored(&self) -> Vec<MonRecord> {
        self.lock().mon.clone()
    }

    /// `SM_MON`.
    pub fn monitor(&self, r: MonRecord) {
        let mut g = self.lock();
        g.mon.retain(|m| {
            !(same_host(&m.mon_name, &r.mon_name)
                && m.my_name == r.my_name
                && (m.my_prog, m.my_vers, m.my_proc) == (r.my_prog, r.my_vers, r.my_proc))
        });
        g.mon.push(r);
        self.save(&g);
    }

    /// `SM_UNMON`: one host for one local program.
    pub fn unmonitor(&self, mon_name: &[u8], my_name: &[u8], prog: u32, vers: u32, proc_: u32) {
        let mut g = self.lock();
        g.mon.retain(|m| {
            !(same_host(&m.mon_name, mon_name)
                && m.my_name == my_name
                && (m.my_prog, m.my_vers, m.my_proc) == (prog, vers, proc_))
        });
        self.save(&g);
    }

    /// `SM_UNMON_ALL`: every host for one local program.
    pub fn unmonitor_all(&self, my_name: &[u8], prog: u32, vers: u32, proc_: u32) {
        let mut g = self.lock();
        g.mon.retain(|m| {
            !(m.my_name == my_name && (m.my_prog, m.my_vers, m.my_proc) == (prog, vers, proc_))
        });
        self.save(&g);
    }

    /// `SM_SIMU_CRASH`: as a restart, the state up by two and the hosts
    /// to tell handed back.
    pub fn simu_crash(&self) -> Vec<MonRecord> {
        let mut g = self.lock();
        g.state = next_state(Some(g.state));
        if let Err(e) = self.store.write_state(&encode_state(g.state)) {
            tracing::warn!("state not written: {e}");
        }
        let out = std::mem::take(&mut g.mon);
        self.save(&g);
        out
    }

    /// A host (`name`, from `peer`) said it restarted in `state`: every
    /// local program monitoring it is called back. Its records stay
    /// (its new locks are monitored the same way).
    pub fn notified(&self, name: &[u8], peer: IpAddr, state: i32) -> usize {
        let ip = peer.to_string();
        let hit: Vec<MonRecord> = self
            .lock()
            .mon
            .iter()
            .filter(|m| same_host(&m.mon_name, name) || m.mon_name == ip.as_bytes())
            .cloned()
            .collect();
        tracing::info!(
            host = %String::from_utf8_lossy(name),
            %peer,
            state,
            callbacks = hit.len(),
            "a monitored host restarted"
        );
        for m in &hit {
            self.call_back(m, state);
        }
        hit.len()
    }

    fn call_back(&self, m: &MonRecord, state: i32) {
        let host = std::str::from_utf8(&m.my_name)
            .ok()
            .and_then(|n| n.parse::<IpAddr>().ok())
            .map_or(Host::Addr(IpAddr::from([127, 0, 0, 1])), Host::Addr);
        let Some(addr) = self
            .cfg
            .locator
            .find(&host, m.my_prog, m.my_vers, Proto::Udp)
        else {
            tracing::warn!("no program {} {} to call back", m.my_prog, m.my_vers);
            return;
        };
        let st = status {
            mon_name: m.mon_name.clone(),
            state,
            r#priv: m.private,
        };
        let Ok(args) = xdr::to_bytes(&st) else { return };
        let mut c = RpcClient::new(addr, Proto::Udp, self.cfg.rpc.clone());
        if let Err(e) = c.call_raw(m.my_prog, m.my_vers, m.my_proc, &Credential::None, &args) {
            tracing::warn!("call back to {addr} failed: {e}");
        }
    }

    /// Tells each host in `hosts` this host restarted, in rounds until
    /// each answered, the rounds are over or `stop` is set: the hosts
    /// never reached.
    pub fn notify_hosts(&self, hosts: &[MonRecord], stop: &AtomicBool) -> Vec<Vec<u8>> {
        let mut left: Vec<Vec<u8>> = Vec::new();
        for h in hosts {
            if !left.iter().any(|x| same_host(x, &h.mon_name)) {
                left.push(h.mon_name.clone());
            }
        }
        let state = self.state_i32();
        for round in 0..self.cfg.notify_rounds.max(1) {
            if stop.load(Ordering::Relaxed) || left.is_empty() {
                break;
            }
            if round > 0 {
                std::thread::sleep(self.cfg.notify_pause);
            }
            left.retain(|name| !self.notify_one(name, state));
        }
        for name in &left {
            tracing::warn!(host = %String::from_utf8_lossy(name), "a host was not told of this restart");
        }
        left
    }

    fn notify_one(&self, name: &[u8], state: i32) -> bool {
        let n = String::from_utf8_lossy(name).into_owned();
        let host = n.parse::<IpAddr>().map_or(Host::Name(n), Host::Addr);
        let Some(addr) = self.cfg.locator.find(&host, SM_PROG::NUMBER, 1, Proto::Udp) else {
            return false;
        };
        let told =
            super::call::sm_notify(addr, &self.cfg.rpc, self.cfg.me.as_bytes(), state).is_ok();
        if told {
            tracing::info!(host = %String::from_utf8_lossy(name), %addr, state, "told of this restart");
        }
        told
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_go_up_odd() {
        assert_eq!(next_state(None), 1);
        assert_eq!(next_state(Some(1)), 3);
        assert_eq!(next_state(Some(4)), 5);
        assert_eq!(next_state(Some(u32::MAX)), 1);
    }
}
