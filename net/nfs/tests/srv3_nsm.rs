// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The status monitor: the state number across starts in `/etc/STATD`'s
//! form, monitoring from this host only, `SM_UNMON`, `SM_UNMON_ALL`,
//! `SM_SIMU_CRASH`, and `SM_NOTIFY` to the hosts monitored before a start.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nfs::nsm::Nsm;
use nfs::nsm::record::{
    MemStatdStore, MonRecord, decode_monitored, decode_state, encode_monitored, encode_state,
};
use nfs::nsm::state::{NsmConfig, NsmState};
use nfs::rpc::{
    AcceptError, Answer, CallContext, ClientConfig, Dispatcher, Program, Transport, decode_args,
    encode_res, serve_datagram,
};
use nfs::server::locate::{FixedLocator, Host, Locator};
use nfs::types::nsm::SM_PROG::SM_VERS::{Args, Res};
use nfs::types::nsm::{mon, mon_id, my_id, res, sm_name, stat_chge};
use nfs::xdr::Decoder;
use srv3_common::Client;

/// Another host's monitor: the notices it got.
#[derive(Default)]
struct Remote(Mutex<Vec<stat_chge>>);

impl Program for Remote {
    fn program(&self) -> u32 {
        100_024
    }
    fn versions(&self) -> &[u32] {
        &[1]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        match decode_args::<Args>(ctx, d)? {
            Args::SM_NOTIFY(c) => {
                self.0.lock().unwrap().push(c);
                encode_res(&Res::SM_NOTIFY)
            }
            _ => Err(AcceptError::ProcUnavail),
        }
    }
}

fn config(locator: Arc<FixedLocator>) -> NsmConfig {
    NsmConfig {
        me: "qs400".into(),
        locator: locator as Arc<dyn Locator>,
        rpc: ClientConfig {
            timeout: Duration::from_millis(300),
            retransmits: 1,
            reserved_port: false,
            ..ClientConfig::default()
        },
        notify_rounds: 2,
        notify_pause: Duration::from_millis(10),
    }
}

fn mon_of(host: &str) -> mon {
    mon {
        mon_id: mon_id {
            mon_name: host.as_bytes().to_vec(),
            my_id: my_id {
                my_name: b"127.0.0.1".to_vec(),
                my_prog: 100_021,
                my_vers: 2,
                my_proc: 1,
            },
        },
        r#priv: [5; 16],
    }
}

/// A store the test looks into while the monitor writes it.
struct Shared(Arc<MemStatdStore>);
impl nfs::nsm::record::StatdStore for Shared {
    fn read_state(&self) -> std::io::Result<Option<Vec<u8>>> {
        self.0.read_state()
    }
    fn write_state(&self, b: &[u8]) -> std::io::Result<()> {
        self.0.write_state(b)
    }
    fn read_monitored(&self) -> std::io::Result<Option<Vec<u8>>> {
        self.0.read_monitored()
    }
    fn write_monitored(&self, b: &[u8]) -> std::io::Result<()> {
        self.0.write_monitored(b)
    }
}

#[test]
fn the_state_goes_up_by_two_and_monitoring_is_local() {
    let store = Arc::new(MemStatdStore::default());
    *store.state.lock().unwrap() = Some(vec![0xF1, 0x15]);
    let loc = Arc::new(FixedLocator::new());
    let (st, before) = NsmState::start(
        Box::new(Shared(Arc::clone(&store))),
        config(Arc::clone(&loc)),
    )
    .unwrap();
    assert!(before.is_empty());
    assert_eq!(st.state(), 3);
    assert_eq!(
        store.state.lock().unwrap().as_deref(),
        Some(&[0xF3, 0x15][..])
    );
    let p = Nsm::new(Arc::clone(&st));
    let mut d = Dispatcher::new();
    d.add(Arc::new(p.clone()));
    let local = Client::new("127.0.0.1:800", 0);
    let far = Client::new("10.0.0.9:800", 0);
    match local.call(
        &p,
        &Args::SM_STAT(sm_name {
            mon_name: b"x".to_vec(),
        }),
    ) {
        Res::SM_STAT(r) => assert_eq!((r.res_stat, r.state), (res::stat_succ, 3)),
        r => panic!("{r:?}"),
    }
    match far.call(&p, &Args::SM_MON(mon_of("evil"))) {
        Res::SM_MON(r) => assert_eq!(r.res_stat, res::stat_fail),
        r => panic!("{r:?}"),
    }
    for h in ["a", "b"] {
        match local.call(&p, &Args::SM_MON(mon_of(h))) {
            Res::SM_MON(r) => assert_eq!((r.res_stat, r.state), (res::stat_succ, 3)),
            r => panic!("{r:?}"),
        }
    }
    // The same host twice is one record; the record is kept.
    let _: Res = local.call(&p, &Args::SM_MON(mon_of("A")));
    assert_eq!(st.monitored().len(), 2);
    let kept = decode_monitored(store.monitored.lock().unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].private, [5; 16]);
    let _: Res = far.call(&p, &Args::SM_UNMON(mon_of("b").mon_id));
    assert_eq!(st.monitored().len(), 2, "not from another host");
    let _: Res = local.call(&p, &Args::SM_UNMON(mon_of("b").mon_id));
    assert_eq!(st.monitored().len(), 1);
    let _: Res = local.call(&p, &Args::SM_UNMON_ALL(mon_of("a").mon_id.my_id));
    assert!(st.monitored().is_empty());
    // SM_SIMU_CRASH: not from another host.
    let m = far.message(9, &Args::SM_SIMU_CRASH);
    let r = far.send(&p, &m).unwrap();
    assert!(matches!(
        nfs::rpc::client::results(&r),
        Err(nfs::rpc::RpcError::Refused(AcceptError::ProcUnavail))
    ));
    let _: Res = local.call(&p, &Args::SM_SIMU_CRASH);
    assert_eq!(st.state(), 5);
    assert_eq!(
        decode_state(store.state.lock().unwrap().as_ref().unwrap()),
        Some(5)
    );
    let _ = (Transport::Udp, IpAddr::from([0, 0, 0, 0]));
}

#[test]
fn a_start_tells_the_hosts_monitored_before() {
    let remote = Arc::new(Remote::default());
    let u = UdpSocket::bind("127.0.0.1:0").unwrap();
    u.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let addr: SocketAddr = u.local_addr().unwrap();
    let mut d = Dispatcher::new();
    d.add(Arc::clone(&remote) as Arc<dyn Program>);
    let loc = Arc::new(FixedLocator::new());
    loc.put(Host::Name("client1".into()), 100_024, 1, addr);
    let store = MemStatdStore::default();
    *store.state.lock().unwrap() = Some(encode_state(7));
    let rec = |h: &str| MonRecord {
        mon_name: h.as_bytes().to_vec(),
        my_name: b"127.0.0.1".to_vec(),
        my_prog: 100_021,
        my_vers: 2,
        my_proc: 1,
        private: [0; 16],
    };
    *store.monitored.lock().unwrap() = Some(encode_monitored(&[
        rec("client1"),
        rec("CLIENT1"),
        rec("gone"),
    ]));
    let (st, before) = NsmState::start(Box::new(store), config(Arc::clone(&loc))).unwrap();
    assert_eq!(st.state(), 9);
    assert_eq!(before.len(), 3);
    assert!(st.monitored().is_empty(), "the new start monitors anew");
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            while !stop.load(Ordering::Relaxed) {
                let _ = serve_datagram(&u, &d);
            }
        });
        let left = st.notify_hosts(&before, &AtomicBool::new(false));
        stop.store(true, Ordering::SeqCst);
        // `gone` has no monitor to reach; client1 is told once.
        assert_eq!(left, vec![b"gone".to_vec()]);
    });
    let got = remote.0.lock().unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].mon_name.as_slice(), got[0].state),
        (&b"qs400"[..], 9)
    );
}
