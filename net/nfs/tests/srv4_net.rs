// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `run::nfsd4` over loopback TCP: version 3 and 4 on one port, and
//! NFSv4.1's backchannel on the client's own connection: a session
//! made with `CREATE_SESSION4_FLAG_CONN_BACK_CHAN`, a read delegation,
//! a local job's write breaking it, `CB_SEQUENCE` and `CB_RECALL` sent
//! on the connection the client opened, answered there, and the
//! delegation returned.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::O_WRONLY;
use nfs::rpc::msg::{encode_call, reply_success};
use nfs::rpc::record::{frame, read_record};
use nfs::rpc::{ClientConfig, Credential, Proto, RpcClient};
use nfs::server::news::{ClientNews, NewsSource};
use nfs::server::run::{self, Common, Listeners};
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3;
use nfs::types::nfs4::{
    self as t, CB_COMPOUND4args, CB_COMPOUND4res, CB_RECALL4res, CB_SEQUENCE4res,
    CB_SEQUENCE4resok, CREATE_SESSION4_FLAG_CONN_BACK_CHAN, nfs_argop4, nfs_cb_argop4,
    nfs_cb_resop4, nfs_resop4, nfsstat4,
};
use nfs::types::rpc::call_body;
use nfs::xdr;
use srv3_common::World;
use srv4_common::{Env, create_session, exchange_id, getfh, lookup, rootfh};

/// A 4.1 client on one TCP connection that answers the server's calls.
struct Conn {
    s: TcpStream,
    xid: u32,
    /// The callbacks it answered: (sequence ID, recalled stateid).
    got: Vec<(u32, t::stateid4)>,
}

impl Conn {
    fn new(addr: SocketAddr) -> Self {
        let s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Self {
            s,
            xid: 1,
            got: Vec::new(),
        }
    }

    /// Answers one call from the server.
    fn answer(&mut self, m: &[u8]) {
        let call = nfs::rpc::msg::parse_call(m).unwrap();
        assert_eq!(
            call.body.prog, 0x4000_0002,
            "the session's callback program"
        );
        let mut d = call.args;
        let a: CB_COMPOUND4args = xdr::from_bytes(d.rest()).unwrap();
        assert_eq!(a.minorversion, 1);
        let mut res = Vec::new();
        let mut seq = 0;
        for op in a.argarray {
            match op {
                nfs_cb_argop4::OP_CB_SEQUENCE(s) => {
                    seq = s.csa_sequenceid;
                    res.push(nfs_cb_resop4::OP_CB_SEQUENCE(CB_SEQUENCE4res::NFS4_OK(
                        CB_SEQUENCE4resok {
                            csr_sessionid: s.csa_sessionid,
                            csr_sequenceid: s.csa_sequenceid,
                            csr_slotid: 0,
                            csr_highest_slotid: 0,
                            csr_target_highest_slotid: 0,
                        },
                    )));
                }
                nfs_cb_argop4::OP_CB_RECALL(r) => {
                    self.got.push((seq, r.stateid));
                    res.push(nfs_cb_resop4::OP_CB_RECALL(CB_RECALL4res {
                        status: nfsstat4::NFS4_OK,
                    }));
                }
                o => panic!("{o:?}"),
            }
        }
        let body = xdr::to_bytes(&CB_COMPOUND4res {
            status: nfsstat4::NFS4_OK,
            tag: a.tag,
            resarray: res,
        })
        .unwrap();
        self.s
            .write_all(&frame(&reply_success(call.xid, &body)))
            .unwrap();
    }

    /// Reads until the reply to `xid`, answering calls on the way.
    fn reply(&mut self, xid: u32) -> Vec<u8> {
        loop {
            let m = read_record(&mut self.s, 2 << 20).unwrap().unwrap();
            if nfs::rpc::back::is_reply(&m) {
                if m[..4] == xid.to_be_bytes() {
                    return nfs::rpc::client::results(&m).unwrap();
                }
                continue;
            }
            self.answer(&m);
        }
    }

    /// Waits for a call from the server and answers it.
    fn serve_one(&mut self) {
        let m = read_record(&mut self.s, 2 << 20).unwrap().unwrap();
        assert!(!nfs::rpc::back::is_reply(&m));
        self.answer(&m);
    }

    fn compound(&mut self, ops: Vec<nfs_argop4>) -> t::COMPOUND4res {
        self.xid += 1;
        let xid = self.xid;
        let args = xdr::to_bytes(&t::COMPOUND4args {
            tag: t::utf8string(Vec::new()),
            minorversion: 1,
            argarray: ops,
        })
        .unwrap();
        let body = call_body {
            rpcvers: 2,
            prog: 100_003,
            vers: 4,
            proc: 1,
            cred: Credential::sys(b"c", 100, 100, &[]).to_opaque().unwrap(),
            verf: nfs::rpc::auth::none(),
        };
        let m = encode_call(xid, &body, &args).unwrap();
        self.s.write_all(&frame(&m)).unwrap();
        xdr::from_bytes(&self.reply(xid)).unwrap()
    }
}

#[test]
fn version_three_and_the_four_one_backchannel() {
    let w = World::new();
    let x = w.export(1, "/home/x");
    w.put(x);
    w.write_file("/home/x/f", b"data");
    w.job(9).chmod("/home/x/f", 0o666).unwrap();
    let e = Env::new(w);
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = tcp.local_addr().unwrap();
    let stop = AtomicBool::new(false);
    let ready = AtomicUsize::new(0);
    let on_ready = || {
        ready.fetch_add(1, Ordering::SeqCst);
    };
    std::thread::scope(|sc| {
        let (w, srv, stop) = (&e.w, &e.srv, &stop);
        sc.spawn(|| {
            run::nfsd4(
                Common {
                    listeners: Listeners {
                        tcp: Some(tcp),
                        udp: None,
                    },
                    binder: None,
                    stop,
                    ready: Some(&on_ready),
                },
                srv,
                || w.vfs(),
                || Box::new(ClientNews(w.job(1))) as Box<dyn NewsSource>,
            )
            .unwrap();
        });
        let body = || {
            let end = Instant::now() + Duration::from_secs(10);
            while ready.load(Ordering::SeqCst) == 0 {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(10));
            }
            // Version 3 on the same port.
            let mut v3 = RpcClient::new(addr, Proto::Tcp, ClientConfig::default());
            let r: NFS_V3::Res = v3
                .call_proc(&Credential::None, &NFS_V3::Args::NFSPROC3_NULL)
                .unwrap();
            assert_eq!(r, NFS_V3::Res::NFSPROC3_NULL);
            // A 4.1 session with its backchannel on this connection.
            let mut c = Conn::new(addr);
            let r = c.compound(vec![exchange_id("net", [1; 8], 0)]);
            let nfs_resop4::OP_EXCHANGE_ID(t::EXCHANGE_ID4res::NFS4_OK(ex)) = &r.resarray[0] else {
                panic!("{r:?}")
            };
            let r = c.compound(vec![create_session(
                ex.eir_clientid,
                ex.eir_sequenceid,
                CREATE_SESSION4_FLAG_CONN_BACK_CHAN,
            )]);
            let nfs_resop4::OP_CREATE_SESSION(t::CREATE_SESSION4res::NFS4_OK(cs)) = &r.resarray[0]
            else {
                panic!("{r:?}")
            };
            assert_eq!(cs.csr_flags, CREATE_SESSION4_FLAG_CONN_BACK_CHAN);
            let sid = cs.csr_sessionid;
            let seq = |n| {
                nfs_argop4::OP_SEQUENCE(t::SEQUENCE4args {
                    sa_sessionid: sid,
                    sa_sequenceid: n,
                    sa_slotid: 0,
                    sa_highest_slotid: 0,
                    sa_cachethis: false,
                })
            };
            let r = c.compound(vec![
                seq(1),
                rootfh(),
                lookup("home"),
                lookup("x"),
                srv4_common::open_args(
                    0,
                    t::state_owner4 {
                        clientid: ex.eir_clientid,
                        owner: b"o".to_vec(),
                    },
                    "f",
                    false,
                    1 | t::OPEN4_SHARE_ACCESS_WANT_READ_DELEG,
                    0,
                ),
                getfh(),
            ]);
            assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
            let nfs_resop4::OP_SEQUENCE(t::SEQUENCE4res::NFS4_OK(s)) = &r.resarray[0] else {
                panic!()
            };
            assert_eq!(s.sr_status_flags & t::SEQ4_STATUS_CB_PATH_DOWN, 0);
            let o = srv4_common::opened(&r);
            let fh = srv4_common::fh_of(&r);
            let t::open_delegation4::OPEN_DELEGATE_READ(d) = &o.delegation else {
                panic!("{:?}", o.delegation)
            };
            // A local job writes: CB_RECALL arrives on this connection.
            let j = e.w.job(5);
            let fd = j.open("/home/x/f", OpenArgs::new(O_WRONLY)).unwrap();
            j.pwrite(fd, b"X", 0).unwrap();
            j.close(fd).unwrap();
            c.serve_one();
            assert_eq!(c.got.len(), 1);
            assert_eq!(c.got[0].0, 1, "the back slot's first sequence ID");
            assert_eq!(c.got[0].1, d.stateid);
            let r = c.compound(vec![
                seq(2),
                srv4_common::putfh(&fh),
                nfs_argop4::OP_DELEGRETURN(t::DELEGRETURN4args {
                    deleg_stateid: d.stateid.clone(),
                }),
            ]);
            assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
            assert_eq!(e.srv.delegations(), 0);
        };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        stop.store(true, Ordering::SeqCst);
        if let Err(p) = r {
            std::panic::resume_unwind(p);
        }
    });
}
