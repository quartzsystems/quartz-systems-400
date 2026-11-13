// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lock manager: conflicts between clients and with a local job,
//! a blocking lock granted by a callback, cancel, the grace period and
//! reclaims, shares, the `_MSG` forms, the 32-bit versions, and a
//! client's restart told through the status monitor freeing its locks.

#![allow(clippy::many_single_char_names)]
#![allow(clippy::too_many_lines)]

mod srv3_common;

use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ifs::client::IfsClient;
use ifs::lock::range::RangeKind;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{F_SETLK, O_CREAT, O_RDWR, SEEK_SET};
use nfs::nlm::Nlm;
use nfs::nlm::grant::grant_ready;
use nfs::nlm::state::{NlmConfig, NlmState};
use nfs::nsm::record::MemStatdStore;
use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::rpc::{
    AcceptError, Answer, CallContext, ClientConfig, Credential, Dispatcher, Program, Proto,
    RpcClient, encode_res, serve_datagram,
};
use nfs::server::ifsvfs::IfsVfs;
use nfs::server::locate::{FixedLocator, Host};
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common, Listeners};
use nfs::server::{Clock, ExportTable, ManualClock, SystemClock};
use nfs::types::nlm::NLM_PROG::{NLM_SM, NLM_VERSX, NLM4_VERS};
use nfs::types::nlm::{
    fsh_access, fsh_mode, netobj, nlm_lock, nlm_lockargs, nlm_share, nlm_shareargs, nlm_sm_status,
    nlm_stats, nlm4_cancargs, nlm4_lock, nlm4_lockargs, nlm4_res, nlm4_stat, nlm4_stats,
    nlm4_testargs, nlm4_testrply, nlm4_unlockargs,
};
use nfs::types::nsm::SM_PROG::SM_VERS;
use nfs::types::nsm::stat_chge;
use nfs::xdr::{Decoder, Xdr};
use srv3_common::{Client, World};

const LOCAL: &str = "127.0.0.1:700";

/// Sets the flag when dropped, so a failed assertion still ends the threads.
struct StopOnDrop<'a>(&'a AtomicBool);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// What a client's lock manager received: (version, procedure, args).
#[derive(Default)]
struct Catcher {
    got: Mutex<Vec<(u32, u32, Vec<u8>)>>,
    take: AtomicBool,
}

impl Program for Catcher {
    fn program(&self) -> u32 {
        100_021
    }
    fn versions(&self) -> &[u32] {
        &[1, 3, 4]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        let args = d.rest().to_vec();
        self.got
            .lock()
            .unwrap()
            .push((ctx.vers, ctx.procedure, args.clone()));
        if ctx.procedure == 5 && ctx.vers == 4 {
            let a = nlm4_testargs::decode(&mut Decoder::new(&args))
                .map_err(|_| AcceptError::GarbageArgs)?;
            let stat = if self.take.load(Ordering::SeqCst) {
                nlm4_stats::NLM4_GRANTED
            } else {
                nlm4_stats::NLM4_DENIED
            };
            return encode_res(&NLM4_VERS::Res::NLMPROC4_GRANTED(nlm4_res {
                cookie: a.cookie,
                stat: nlm4_stat { stat },
            }));
        }
        Ok(Some(Vec::new()))
    }
}

fn cfg(
    locator: Arc<FixedLocator>,
    clock: Arc<dyn Clock>,
    grace: Duration,
    monitor: bool,
) -> NlmConfig {
    NlmConfig {
        monitor,
        locator,
        rpc: ClientConfig {
            timeout: Duration::from_millis(500),
            retransmits: 1,
            reserved_port: false,
            ..ClientConfig::default()
        },
        grace,
        grant_tries: 2,
        clock,
    }
}

fn lock(name: &str, svid: i32, fh: &[u8], off: u64, len: u64) -> nlm4_lock {
    nlm4_lock {
        caller_name: name.as_bytes().to_vec(),
        fh: netobj(fh.to_vec()),
        oh: netobj(format!("oh-{name}-{svid}").into_bytes()),
        svid,
        l_offset: off,
        l_len: len,
    }
}

fn lockargs(l: nlm4_lock, block: bool, reclaim: bool) -> NLM4_VERS::Args {
    NLM4_VERS::Args::NLMPROC4_LOCK(nlm4_lockargs {
        cookie: netobj(b"c".to_vec()),
        block,
        exclusive: true,
        alock: l,
        reclaim,
        state: 1,
    })
}

fn stat_of(r: &NLM4_VERS::Res) -> nlm4_stats {
    match r {
        NLM4_VERS::Res::NLMPROC4_LOCK(x)
        | NLM4_VERS::Res::NLMPROC4_UNLOCK(x)
        | NLM4_VERS::Res::NLMPROC4_CANCEL(x)
        | NLM4_VERS::Res::NLMPROC4_NM_LOCK(x) => x.stat.stat,
        NLM4_VERS::Res::NLMPROC4_TEST(t) => match &t.test_stat {
            nlm4_testrply::NLM4_DENIED(_) => nlm4_stats::NLM4_DENIED,
            nlm4_testrply::default(s) => *s,
        },
        NLM4_VERS::Res::NLMPROC4_SHARE(s) | NLM4_VERS::Res::NLMPROC4_UNSHARE(s) => s.stat,
        r => panic!("{r:?}"),
    }
}

fn test_args(l: nlm4_lock) -> NLM4_VERS::Args {
    NLM4_VERS::Args::NLMPROC4_TEST(nlm4_testargs {
        cookie: netobj(b"t".to_vec()),
        exclusive: true,
        alock: l,
    })
}

fn unlock_args(l: nlm4_lock) -> NLM4_VERS::Args {
    NLM4_VERS::Args::NLMPROC4_UNLOCK(nlm4_unlockargs {
        cookie: netobj(b"u".to_vec()),
        alock: l,
    })
}

struct Setup {
    w: World,
    file: Vec<u8>,
    locator: Arc<FixedLocator>,
}

fn setup() -> Setup {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    w.write_file("/home/exp/f", b"0123456789");
    w.job(9).chmod("/home/exp/f", 0o666).unwrap();
    let file = w.handle(1, "/home/exp/f");
    Setup {
        w,
        file,
        locator: Arc::new(FixedLocator::new()),
    }
}

fn nlm(
    s: &Setup,
    st: &Arc<NlmState>,
) -> Nlm<IfsVfs<ifs::client::LocalIfs<ifs::backend::mem::MemBackend>>> {
    Nlm::new(s.w.vfs(), s.w.gate(), Arc::clone(st))
}

fn client(peer: &str) -> Client {
    let mut c = Client::new(peer, 100);
    c.d = Dispatcher::new();
    c.d.set_cache(run::nlm_cache());
    c
}

#[test]
fn two_clients_conflict_and_a_local_job_too() {
    let s = setup();
    let st = NlmState::new(cfg(
        Arc::clone(&s.locator),
        Arc::new(SystemClock),
        Duration::ZERO,
        false,
    ));
    let p = nlm(&s, &st);
    let c = client("10.0.0.5:700");
    let a = lock("clientA", 1, &s.file, 0, 10);
    let b = lock("clientB", 7, &s.file, 5, 1);
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(a.clone(), false, false))),
        nlm4_stats::NLM4_GRANTED
    );
    // B: TEST names A's lock.
    match c.call(&p, &test_args(b.clone())) {
        NLM4_VERS::Res::NLMPROC4_TEST(t) => match t.test_stat {
            nlm4_testrply::NLM4_DENIED(h) => {
                assert_eq!(
                    (h.svid, h.oh.0, h.l_offset, h.l_len, h.exclusive),
                    (1, a.oh.0.clone(), 0, 10, true)
                );
            }
            x @ nlm4_testrply::default(_) => panic!("{x:?}"),
        },
        r => panic!("{r:?}"),
    }
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(b.clone(), false, false))),
        nlm4_stats::NLM4_DENIED
    );
    // The same owner again: no conflict with itself.
    assert_eq!(
        stat_of(&c.call(
            &p,
            &lockargs(lock("clientA", 1, &s.file, 2, 2), false, false)
        )),
        nlm4_stats::NLM4_GRANTED
    );
    assert_eq!(
        stat_of(&c.call(&p, &unlock_args(a.clone()))),
        nlm4_stats::NLM4_GRANTED
    );
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(b.clone(), false, false))),
        nlm4_stats::NLM4_GRANTED
    );
    assert_eq!(
        stat_of(&c.call(&p, &unlock_args(b.clone()))),
        nlm4_stats::NLM4_GRANTED
    );
    // A local job's lock is in the way too.
    let j = s.w.job(2);
    let fd = j
        .open("/home/exp/f", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    j.lock_range(fd, F_SETLK, RangeKind::Write, SEEK_SET, 0, 4)
        .unwrap();
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(a.clone(), false, false))),
        nlm4_stats::NLM4_DENIED
    );
    assert_eq!(
        stat_of(&c.call(&p, &test_args(a.clone()))),
        nlm4_stats::NLM4_DENIED
    );
    j.close(fd).unwrap();
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(a, false, false))),
        nlm4_stats::NLM4_GRANTED
    );
    // A handle that is no longer good.
    assert_eq!(
        stat_of(&c.call(
            &p,
            &lockargs(lock("clientA", 1, b"junk", 0, 1), false, false)
        )),
        nlm4_stats::NLM4_STALE_FH
    );
}

/// Serves `catcher` on a loopback UDP port until `stop`.
fn serve_catcher<'s>(
    sc: &'s std::thread::Scope<'s, '_>,
    catcher: Arc<Catcher>,
    stop: &'s AtomicBool,
) -> SocketAddr {
    let u = UdpSocket::bind("127.0.0.1:0").unwrap();
    u.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let addr = u.local_addr().unwrap();
    let mut d = Dispatcher::new();
    d.add(catcher);
    sc.spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let _ = serve_datagram(&u, &d);
        }
    });
    addr
}

#[test]
fn a_blocked_lock_is_granted_by_callback_or_cancelled() {
    let s = setup();
    let st = NlmState::new(cfg(
        Arc::clone(&s.locator),
        Arc::new(SystemClock),
        Duration::ZERO,
        false,
    ));
    let p = nlm(&s, &st);
    let c = client(LOCAL);
    let stop = AtomicBool::new(false);
    let catcher = Arc::new(Catcher::default());
    catcher.take.store(true, Ordering::SeqCst);
    std::thread::scope(|sc| {
        let _guard = StopOnDrop(&stop);
        let addr = serve_catcher(sc, Arc::clone(&catcher), &stop);
        s.locator
            .put(Host::Addr(IpAddr::from([127, 0, 0, 1])), 100_021, 4, addr);
        let a = lock("clientA", 1, &s.file, 0, 10);
        let b = lock("clientB", 2, &s.file, 0, 10);
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(a.clone(), false, false))),
            nlm4_stats::NLM4_GRANTED
        );
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(b.clone(), true, false))),
            nlm4_stats::NLM4_BLOCKED
        );
        // Asked again while it waits: still blocked, queued once.
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(b.clone(), true, false))),
            nlm4_stats::NLM4_BLOCKED
        );
        assert_eq!(st.blocked_count(), 1);
        assert_eq!(
            stat_of(&c.call(&p, &unlock_args(a.clone()))),
            nlm4_stats::NLM4_GRANTED
        );
        // The job's poll names the token; the grant goes to the client.
        let vfs = s.w.vfs();
        let mut news = ClientNews(s.w.job(1));
        let n = news.poll(1000).unwrap();
        assert_eq!(n.locks.len(), 1);
        for t in n.locks {
            st.granted_token(t);
        }
        assert_eq!(grant_ready(&st, &vfs), 1);
        let got = catcher.got.lock().unwrap().clone();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].0, got[0].1), (4, 5));
        let g = nlm4_testargs::decode(&mut Decoder::new(&got[0].2)).unwrap();
        assert_eq!(g.alock.oh.0, b.oh.0);
        // B holds it now.
        assert_eq!(
            stat_of(&c.call(&p, &test_args(a.clone()))),
            nlm4_stats::NLM4_DENIED
        );
        // A waits, then cancels: nothing granted when B unlocks.
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(a.clone(), true, false))),
            nlm4_stats::NLM4_BLOCKED
        );
        let cancel = NLM4_VERS::Args::NLMPROC4_CANCEL(nlm4_cancargs {
            cookie: netobj(b"x".to_vec()),
            block: true,
            exclusive: true,
            alock: a.clone(),
        });
        assert_eq!(stat_of(&c.call(&p, &cancel)), nlm4_stats::NLM4_GRANTED);
        assert_eq!(
            stat_of(&c.call(&p, &cancel)),
            nlm4_stats::NLM4_DENIED,
            "nothing left to cancel"
        );
        assert_eq!(
            stat_of(&c.call(&p, &unlock_args(b.clone()))),
            nlm4_stats::NLM4_GRANTED
        );
        assert!(news.poll(0).unwrap().locks.is_empty());
        assert_eq!(
            stat_of(&c.call(&p, &test_args(a.clone()))),
            nlm4_stats::NLM4_GRANTED
        );
        // A grant the client refuses is given back.
        catcher.take.store(false, Ordering::SeqCst);
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(a.clone(), false, false))),
            nlm4_stats::NLM4_GRANTED
        );
        assert_eq!(
            stat_of(&c.call(&p, &lockargs(b.clone(), true, false))),
            nlm4_stats::NLM4_BLOCKED
        );
        assert_eq!(
            stat_of(&c.call(&p, &unlock_args(a.clone()))),
            nlm4_stats::NLM4_GRANTED
        );
        for t in news.poll(1000).unwrap().locks {
            st.granted_token(t);
        }
        assert_eq!(grant_ready(&st, &vfs), 0);
        assert_eq!(
            stat_of(&c.call(&p, &test_args(a.clone()))),
            nlm4_stats::NLM4_GRANTED
        );
        stop.store(true, Ordering::SeqCst);
    });
}

#[test]
fn the_grace_period_takes_reclaims_only() {
    let s = setup();
    let clock = Arc::new(ManualClock::default());
    let st = NlmState::new(cfg(
        Arc::clone(&s.locator),
        Arc::clone(&clock) as Arc<dyn Clock>,
        Duration::from_secs(90),
        false,
    ));
    let p = nlm(&s, &st);
    let c = client("10.0.0.5:700");
    let a = lock("clientA", 1, &s.file, 0, 10);
    let b = lock("clientB", 1, &s.file, 20, 10);
    assert!(st.in_grace());
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(b.clone(), false, false))),
        nlm4_stats::NLM4_DENIED_GRACE_PERIOD
    );
    assert_eq!(
        stat_of(&c.call(&p, &test_args(b.clone()))),
        nlm4_stats::NLM4_DENIED_GRACE_PERIOD
    );
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(a.clone(), false, true))),
        nlm4_stats::NLM4_GRANTED
    );
    // UNLOCK is taken in the grace period.
    assert_eq!(
        stat_of(&c.call(&p, &unlock_args(a.clone()))),
        nlm4_stats::NLM4_GRANTED
    );
    clock.advance(Duration::from_secs(91));
    assert!(!st.in_grace());
    assert_eq!(
        stat_of(&c.call(&p, &lockargs(b, false, false))),
        nlm4_stats::NLM4_GRANTED
    );
}

#[test]
fn shares_msg_forms_and_32_bit_versions() {
    let s = setup();
    let st = NlmState::new(cfg(
        Arc::clone(&s.locator),
        Arc::new(SystemClock),
        Duration::ZERO,
        false,
    ));
    let p = nlm(&s, &st);
    let c = client(LOCAL);
    let stop = AtomicBool::new(false);
    let catcher = Arc::new(Catcher::default());
    std::thread::scope(|sc| {
        let _guard = StopOnDrop(&stop);
        let addr = serve_catcher(sc, Arc::clone(&catcher), &stop);
        s.locator
            .put(Host::Addr(IpAddr::from([127, 0, 0, 1])), 100_021, 4, addr);
        s.locator
            .put(Host::Addr(IpAddr::from([127, 0, 0, 1])), 100_021, 3, addr);
        // LOCK_MSG: no results now, LOCK_RES to the client's lock manager.
        let m = NLM4_VERS::Args::NLMPROC4_LOCK_MSG(nlm4_lockargs {
            cookie: netobj(b"m1".to_vec()),
            block: false,
            exclusive: true,
            alock: lock("clientA", 1, &s.file, 0, 1),
            reclaim: false,
            state: 1,
        });
        assert_eq!(
            c.call::<_, NLM4_VERS::Res>(&p, &m),
            NLM4_VERS::Res::NLMPROC4_LOCK_MSG
        );
        let got = catcher.got.lock().unwrap().clone();
        assert_eq!((got[0].0, got[0].1), (4, 12));
        let r = nlm4_res::decode(&mut Decoder::new(&got[0].2)).unwrap();
        assert_eq!(
            (r.cookie.0.clone(), r.stat.stat),
            (b"m1".to_vec(), nlm4_stats::NLM4_GRANTED)
        );
        // Version 3: a 32-bit lock and shares.
        let l3 = nlm_lock {
            caller_name: b"clientC".to_vec(),
            fh: netobj(s.file.clone()),
            oh: netobj(b"c".to_vec()),
            svid: 3,
            l_offset: 0,
            l_len: 1,
        };
        let a3 = NLM_VERSX::Args::NLM_LOCK(nlm_lockargs {
            cookie: netobj(b"3".to_vec()),
            block: false,
            exclusive: true,
            alock: l3,
            reclaim: false,
            state: 1,
        });
        match c.call(&p, &a3) {
            NLM_VERSX::Res::NLM_LOCK(r) => assert_eq!(r.stat.stat, nlm_stats::nlm_denied),
            r => panic!("{r:?}"),
        }
        let share = |who: &str, mode, access| {
            NLM_VERSX::Args::NLM_SHARE(nlm_shareargs {
                cookie: netobj(b"s".to_vec()),
                share: nlm_share {
                    caller_name: who.as_bytes().to_vec(),
                    fh: netobj(s.file.clone()),
                    oh: netobj(who.as_bytes().to_vec()),
                    mode,
                    access,
                },
                reclaim: false,
            })
        };
        let sh = |a: &NLM_VERSX::Args| match c.call(&p, a) {
            NLM_VERSX::Res::NLM_SHARE(r) | NLM_VERSX::Res::NLM_UNSHARE(r) => r.stat,
            r => panic!("{r:?}"),
        };
        assert_eq!(
            sh(&share("pc1", fsh_mode::fsm_DW, fsh_access::fsa_RW)),
            nlm_stats::nlm_granted
        );
        assert_eq!(
            sh(&share("pc2", fsh_mode::fsm_DN, fsh_access::fsa_W)),
            nlm_stats::nlm_denied
        );
        assert_eq!(
            sh(&share("pc2", fsh_mode::fsm_DN, fsh_access::fsa_R)),
            nlm_stats::nlm_granted
        );
        let NLM_VERSX::Args::NLM_SHARE(u) = share("pc1", fsh_mode::fsm_DW, fsh_access::fsa_RW)
        else {
            unreachable!()
        };
        assert_eq!(sh(&NLM_VERSX::Args::NLM_UNSHARE(u)), nlm_stats::nlm_granted);
        assert_eq!(
            sh(&share("pc2", fsh_mode::fsm_DN, fsh_access::fsa_W)),
            nlm_stats::nlm_granted
        );
        // A `_RES` this host did not ask for: no lock client here.
        let r = c
            .send(&p, &c.message(5, &NLM4_VERS::Args::NLMPROC4_UNLOCK_RES(r)))
            .unwrap();
        assert!(matches!(
            nfs::rpc::client::results(&r),
            Err(nfs::rpc::RpcError::Refused(AcceptError::ProcUnavail))
        ));
        // FREE_ALL from the host itself drops its locks and shares.
        let mut a_host = client(LOCAL);
        a_host.cred = Credential::sys(b"clientA", 0, 0, &[]);
        let fa = NLM4_VERS::Args::NLMPROC4_FREE_ALL(nfs::types::nlm::nlm4_notify {
            name: b"clientA".to_vec(),
            state: 3,
        });
        let _: NLM4_VERS::Res = a_host.call(&p, &fa);
        assert_eq!(
            stat_of(&c.call(&p, &test_args(lock("clientB", 1, &s.file, 0, 1)))),
            nlm4_stats::NLM4_GRANTED
        );
        stop.store(true, Ordering::SeqCst);
    });
}

fn lo_ip() -> IpAddr {
    IpAddr::from([127, 0, 0, 1])
}

#[test]
fn the_monitors_callback_frees_a_restarted_clients_locks() {
    let s = setup();
    let locator = Arc::clone(&s.locator);
    let nst = NlmState::new(cfg(
        Arc::clone(&locator),
        Arc::new(SystemClock),
        Duration::ZERO,
        true,
    ));
    let store = Box::new(MemStatdStore::default());
    let (sst, before) = NsmState::start(
        store,
        NsmConfig {
            me: "qs400".into(),
            locator: Arc::clone(&locator) as Arc<dyn nfs::server::locate::Locator>,
            rpc: ClientConfig {
                timeout: Duration::from_millis(300),
                retransmits: 1,
                reserved_port: false,
                ..ClientConfig::default()
            },
            notify_rounds: 1,
            notify_pause: Duration::ZERO,
        },
    )
    .unwrap();
    assert!(before.is_empty());
    let stop = AtomicBool::new(false);
    let ready = AtomicUsize::new(0);
    let on_ready = || {
        ready.fetch_add(1, Ordering::SeqCst);
    };
    let nlm_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mon_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let lo = Host::Addr(IpAddr::from([127, 0, 0, 1]));
    locator.put(lo.clone(), 100_024, 1, mon_udp.local_addr().unwrap());
    let nlm_addr = nlm_udp.local_addr().unwrap();
    let mon_addr = mon_udp.local_addr().unwrap();
    // The lock manager registers with a binder; the monitor's callback
    // (NLM_SM, version 2) is found there, as on a system (it was not
    // registered, and no restart notice reached a client's mounts).
    let bsock = UdpSocket::bind("127.0.0.1:0").unwrap();
    bsock
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let baddr = bsock.local_addr().unwrap();
    let mut bd = nfs::rpc::Dispatcher::new();
    bd.add(Arc::new(nfs::rpcb::Binder::default()));
    let exports: Arc<dyn ExportTable> = Arc::clone(&s.w.exports) as Arc<dyn ExportTable>;
    std::thread::scope(|sc| {
        let _guard = StopOnDrop(&stop);
        sc.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                let _ = nfs::rpc::serve_datagram(&bsock, &bd);
            }
        });
        sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: Some(TcpListener::bind("127.0.0.1:0").unwrap()),
                    udp: Some(nlm_udp),
                },
                binder: Some(run::Registration {
                    binder: baddr,
                    owner: "QNFSNLMD".into(),
                }),
                stop: &stop,
                ready: Some(&on_ready),
            };
            run::nlmd(
                c,
                &exports,
                &nst,
                || s.w.vfs(),
                || -> Box<dyn NewsSource> { Box::new(ClientNews(s.w.job(1))) },
            )
            .unwrap();
        });
        sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: None,
                    udp: Some(mon_udp),
                },
                binder: None,
                stop: &stop,
                ready: Some(&on_ready),
            };
            run::nsmd(c, &sst, Vec::new()).unwrap();
        });
        let end = Instant::now() + Duration::from_secs(10);
        while ready.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        let ccfg = ClientConfig {
            timeout: Duration::from_secs(2),
            retransmits: 2,
            reserved_port: false,
            ..ClientConfig::default()
        };
        let mut binder = nfs::rpcb::BinderClient::at(baddr, Proto::Udp, ccfg.clone());
        let sm = binder
            .getport(100_021, 2, 17)
            .unwrap()
            .expect("NLM_SM (version 2) registered with the binder");
        assert_eq!(sm, nlm_addr.port());
        locator.put(lo.clone(), 100_021, 2, SocketAddr::new(lo_ip(), sm));
        let mut n = RpcClient::new(nlm_addr, Proto::Udp, ccfg.clone());
        let cred = Credential::sys(b"clientA", 100, 100, &[]);
        let call = |n: &mut RpcClient, a: &NLM4_VERS::Args| {
            n.call_proc::<NLM4_VERS::Args, NLM4_VERS::Res>(&cred, a)
                .unwrap()
        };
        let a = lock("clientA", 1, &s.file, 0, 10);
        let b = lock("clientB", 1, &s.file, 0, 10);
        assert_eq!(
            stat_of(&call(&mut n, &lockargs(a, false, false))),
            nlm4_stats::NLM4_GRANTED
        );
        // The lock manager asked the monitor to watch clientA.
        let mon = sst.monitored();
        assert_eq!(mon.len(), 1);
        assert_eq!(
            (
                mon[0].mon_name.as_slice(),
                mon[0].my_prog,
                mon[0].my_vers,
                mon[0].my_proc
            ),
            (&b"clientA"[..], 100_021, 2, 1)
        );
        assert_eq!(
            stat_of(&call(&mut n, &lockargs(b.clone(), false, false))),
            nlm4_stats::NLM4_DENIED
        );
        // clientA restarted: its monitor tells ours, ours calls the lock manager back.
        let mut m = RpcClient::new(mon_addr, Proto::Udp, ccfg.clone());
        let r = m
            .call_proc::<SM_VERS::Args, SM_VERS::Res>(
                &Credential::None,
                &SM_VERS::Args::SM_NOTIFY(stat_chge {
                    mon_name: b"CLIENTA".to_vec(),
                    state: 3,
                }),
            )
            .unwrap();
        assert_eq!(r, SM_VERS::Res::SM_NOTIFY);
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if stat_of(&call(&mut n, &lockargs(b.clone(), false, false)))
                == nlm4_stats::NLM4_GRANTED
            {
                break;
            }
            assert!(Instant::now() < end, "clientA's lock was not freed");
            std::thread::sleep(Duration::from_millis(50));
        }
        // NLM_SM from this host is taken (from another, PROC_UNAVAIL).
        let mut far = RpcClient::new(nlm_addr, Proto::Udp, ccfg);
        let e = far.call_proc::<NLM_SM::Args, NLM_SM::Res>(
            &Credential::None,
            &NLM_SM::Args::NLM_SM_NOTIFY(nlm_sm_status {
                mon_name: b"x".to_vec(),
                state: 1,
                r#priv: [0; 16],
            }),
        );
        assert!(e.is_ok());
        stop.store(true, Ordering::SeqCst);
    });
}
