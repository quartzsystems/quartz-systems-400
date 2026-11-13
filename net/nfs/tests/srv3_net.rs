// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The daemons over loopback: `mountd` and `nfsd` register with a
//! binder, a client finds them through it and mounts, writes and reads
//! over TCP and UDP with `RpcClient`; at the stop they unregister.

mod srv3_common;

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nfs::mount::list::MountList;
use nfs::rpc::{ClientConfig, Credential, Dispatcher, Proto, RpcClient, serve_datagram};
use nfs::rpcb::{Binder, BinderClient};
use nfs::server::ExportTable;
use nfs::server::run::{self, Common, Listeners, Registration};
use nfs::types::mount::MOUNT_PROGRAM::MOUNT_V3;
use nfs::types::mount::{dirpath, mountres3};
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3};
use srv3_common::{World, dirop, fh, mode};

/// A binder on a loopback UDP port.
fn binder() -> (UdpSocket, Dispatcher) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    let mut d = Dispatcher::new();
    d.add(Arc::new(Binder::default()));
    (s, d)
}

fn wait_for(n: &AtomicUsize, want: usize) {
    let end = Instant::now() + Duration::from_secs(10);
    while n.load(Ordering::SeqCst) < want {
        assert!(Instant::now() < end, "the daemons did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn cfg() -> ClientConfig {
    ClientConfig {
        timeout: Duration::from_secs(3),
        retransmits: 2,
        reserved_port: false,
        ..ClientConfig::default()
    }
}

#[test]
fn mount_write_read_over_tcp_and_udp() {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let stop = AtomicBool::new(false);
    let started = AtomicUsize::new(0);
    let bstop = AtomicBool::new(false);
    let (bsock, bd) = binder();
    let baddr = bsock.local_addr().unwrap();
    let reg = |owner: &str| Registration {
        binder: baddr,
        owner: owner.into(),
    };
    let listeners = || Listeners {
        tcp: Some(TcpListener::bind("127.0.0.1:0").unwrap()),
        udp: Some(UdpSocket::bind("127.0.0.1:0").unwrap()),
    };
    let ready = || {
        started.fetch_add(1, Ordering::SeqCst);
    };
    let exports: Arc<dyn ExportTable> = Arc::clone(&w.exports) as Arc<dyn ExportTable>;
    let mounts: Arc<dyn MountList> = Arc::clone(&w.mounts) as Arc<dyn MountList>;
    std::thread::scope(|s| {
        s.spawn(|| {
            while !bstop.load(Ordering::Relaxed) {
                let _ = serve_datagram(&bsock, &bd);
            }
        });
        let m = s.spawn(|| {
            let c = Common {
                listeners: listeners(),
                binder: Some(reg("QNFSMNTD")),
                stop: &stop,
                ready: Some(&ready),
            };
            run::mountd(c, &exports, &mounts, || w.vfs()).unwrap();
        });
        let n = s.spawn(|| {
            let c = Common {
                listeners: listeners(),
                binder: Some(reg("QNFSNFSD")),
                stop: &stop,
                ready: Some(&ready),
            };
            run::nfsd(c, || w.nfs3()).unwrap();
        });
        wait_for(&started, 2);
        let mut b = BinderClient::at(baddr, Proto::Udp, cfg());
        let cred = Credential::sys(b"client1", 100, 100, &[]);
        for proto in [Proto::Tcp, Proto::Udp] {
            let p = if proto == Proto::Tcp { 6 } else { 17 };
            let mport = b.getport(100_005, 3, p).unwrap().expect("MOUNT registered");
            let nport = b.getport(100_003, 3, p).unwrap().expect("NFS registered");
            let mut m = RpcClient::new(SocketAddr::from(([127, 0, 0, 1], mport)), proto, cfg());
            let root = match m
                .call_proc::<MOUNT_V3::Args, MOUNT_V3::Res>(&cred, &MOUNT_V3::Args::MOUNTPROC3_MNT(dirpath(b"/home/exp".to_vec())))
                .unwrap()
            {
                MOUNT_V3::Res::MOUNTPROC3_MNT(mountres3::MNT3_OK(ok)) => ok.fhandle.0,
                r => panic!("{r:?}"),
            };
            let mut n = RpcClient::new(SocketAddr::from(([127, 0, 0, 1], nport)), proto, cfg());
            let name = format!("f-{p}");
            let created = n
                .call_proc::<Args, Res>(
                    &cred,
                    &Args::NFSPROC3_CREATE(t3::CREATE3args {
                        r#where: dirop(&root, &name),
                        how: t3::createhow3::GUARDED(mode(0o644)),
                    }),
                )
                .unwrap();
            let f = match created {
                Res::NFSPROC3_CREATE(t3::CREATE3res::NFS3_OK(ok)) => ok.obj.unwrap().data,
                r => panic!("{r:?}"),
            };
            // A 300 KiB write in one call (more than a datagram over TCP only).
            let data: Vec<u8> = (0..(if proto == Proto::Tcp { 300 * 1024 } else { 8000 }))
                .map(|i| u8::try_from(i % 251).unwrap())
                .collect();
            let wr = n
                .call_proc::<Args, Res>(
                    &cred,
                    &Args::NFSPROC3_WRITE(t3::WRITE3args {
                        file: fh(&f),
                        offset: 0,
                        count: u32::try_from(data.len()).unwrap(),
                        stable: t3::stable_how::FILE_SYNC,
                        data: data.clone(),
                    }),
                )
                .unwrap();
            assert!(matches!(wr, Res::NFSPROC3_WRITE(t3::WRITE3res::NFS3_OK(_))));
            let rd = n
                .call_proc::<Args, Res>(
                    &cred,
                    &Args::NFSPROC3_READ(t3::READ3args {
                        file: fh(&f),
                        offset: 0,
                        count: 1 << 20,
                    }),
                )
                .unwrap();
            match rd {
                Res::NFSPROC3_READ(t3::READ3res::NFS3_OK(ok)) => {
                    assert_eq!(ok.data, data);
                    assert!(ok.eof);
                }
                r => panic!("{r:?}"),
            }
        }
        assert_eq!(w.mounts.list().len(), 1, "one host, one directory");
        stop.store(true, Ordering::SeqCst);
        m.join().unwrap();
        n.join().unwrap();
        // Both daemons unregistered at their stop.
        let left: Vec<u32> = b.dump().unwrap().into_iter().map(|r| r.r_prog).collect();
        assert!(!left.contains(&100_003) && !left.contains(&100_005), "{left:?}");
        bstop.store(true, Ordering::SeqCst);
    });
}
