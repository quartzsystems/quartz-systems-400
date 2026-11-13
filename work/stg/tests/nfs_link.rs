// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfs` and `nfs_api` end to end: a daemon's and a command's requests
//! over a link whose far end answers them as the storage service's
//! registry does, both ways through the wire codec.

use std::cell::RefCell;
use std::rc::Rc;

use stg::nfs::{Daemon, NfsDaemonJob, NfsExport, NfsOp, NfsOrder, NfsPort, NfsReply};
use stg::{Client, Link, Request};

#[test]
#[allow(clippy::too_many_lines)] // one conversation
fn a_daemon_and_a_command_over_the_link() {
    let jobs: Rc<RefCell<Vec<NfsDaemonJob>>> = Rc::default();
    let j = Rc::clone(&jobs);
    let mut link = Link::answering(move |req| match req {
        Request::Nfs { op } => {
            let reply = match op {
                NfsOp::Hello { daemon } => {
                    j.borrow_mut().push(NfsDaemonJob {
                        daemon,
                        index: 1,
                        job: "000042/QSECOFR/QNFSNFSD".into(),
                        ready: false,
                        ending: false,
                        ports: Vec::new(),
                        since: 1,
                    });
                    NfsReply::Hello {
                        index: 1,
                        restore: false,
                    }
                }
                NfsOp::Ready { ports } => {
                    if let Some(x) = j.borrow_mut().last_mut() {
                        x.ready = true;
                        x.ports = ports;
                    }
                    NfsReply::Done
                }
                NfsOp::Daemons => NfsReply::Daemons {
                    jobs: j.borrow().clone(),
                },
                NfsOp::Exports => NfsReply::Table {
                    generation: 7,
                    exports: vec![NfsExport {
                        id: 3,
                        entry: "/home/joe -ro,anon=-1".into(),
                        anon_uid: None,
                        anon_profile: "*NONE".into(),
                        by: "000001/QSECOFR/QPADEV0001".into(),
                        since: 2,
                    }],
                },
                NfsOp::Start { daemon, .. } => NfsReply::Refused {
                    msgid: "CPFA1B8".into(),
                    data: vec![0x40; 20],
                    text: format!("{} needs *IOSYSCFG", daemon.keyword()),
                },
                _ => NfsReply::Done,
            };
            ipc::encode(&reply)
        }
        Request::NfsApi { api, parms } => {
            let r: Result<Vec<u8>, (String, Vec<u8>)> = if api == "QZNFRTVE" {
                Ok(parms.concat())
            } else {
                Err(("CPF3C90".into(), Vec::new()))
            };
            ipc::encode(&r)
        }
        other => panic!("not an NFS request: {}", other.name()),
    });
    assert_eq!(
        link.nfs(NfsOp::Hello {
            daemon: Daemon::Svr
        })
        .unwrap(),
        NfsReply::Hello {
            index: 1,
            restore: false
        }
    );
    let port = NfsPort {
        program: 100_003,
        version: 3,
        proto: "udp".into(),
        port: 2049,
    };
    assert_eq!(
        link.nfs(NfsOp::Ready {
            ports: vec![port.clone()]
        })
        .unwrap(),
        NfsReply::Done
    );
    let NfsReply::Daemons { jobs } = link.nfs(NfsOp::Daemons).unwrap() else {
        panic!("the daemons")
    };
    assert!(jobs[0].ready && jobs[0].ports == [port]);
    let NfsReply::Table {
        generation,
        exports,
    } = link.nfs(NfsOp::Exports).unwrap()
    else {
        panic!("the table")
    };
    assert_eq!((generation, exports[0].id), (7, 3));
    let e = nfs_entry(&exports[0].entry);
    assert!(e.0 && e.1 == Some(-1));
    let NfsReply::Refused { msgid, .. } = link
        .nfs(NfsOp::Start {
            daemon: Daemon::Rpc,
            count: 1,
            rtvrpcreg: false,
        })
        .unwrap()
    else {
        panic!("refused")
    };
    assert_eq!(msgid, "CPFA1B8");
    assert_eq!(
        link.nfs_api("QZNFRTVE".into(), vec![vec![1], vec![2, 3]])
            .unwrap(),
        Ok(vec![1, 2, 3])
    );
    assert_eq!(
        link.nfs_api("QZNFNFSO".into(), Vec::new()).unwrap(),
        Err(("CPF3C90".into(), Vec::new()))
    );
}

/// The entry's `ro` and `anon` as the daemons read the line.
fn nfs_entry(line: &str) -> (bool, Option<i64>) {
    let mut words = line.split_whitespace();
    let _path = words.next();
    let opts = words.next().unwrap_or("").trim_start_matches('-');
    let ro = opts.split(',').any(|o| o == "ro");
    let anon = opts
        .split(',')
        .find_map(|o| o.strip_prefix("anon="))
        .and_then(|v| v.parse().ok());
    (ro, anon)
}

/// The lock manager's orders, a daemon's failure and the identity
/// mapping's lookups, both ways through the wire codec.
#[test]
fn orders_failures_and_lookups_over_the_link() {
    let mut link = Link::answering(move |req| match req {
        Request::Nfs { op } => {
            let reply = match op {
                NfsOp::Orders => NfsReply::Orders {
                    orders: vec![NfsOrder {
                        id: 4,
                        host: "client1.lab".into(),
                        by: "000009/QSECOFR/QPADEV0002".into(),
                    }],
                },
                NfsOp::OrderDone { id, count, errno } => NfsReply::Id {
                    id: Some(id * 100 + count + u32::try_from(errno).unwrap_or(9)),
                },
                NfsOp::Failed { errno } => NfsReply::Flag { yes: errno == 98 },
                NfsOp::NameOfUid { uid } => NfsReply::Name {
                    name: (uid == 0).then(|| "QSECOFR".to_string()),
                },
                NfsOp::NameOfGid { .. } => NfsReply::Name { name: None },
                NfsOp::UidOfName { name } => NfsReply::Id {
                    id: (name == "QSYS").then_some(101),
                },
                NfsOp::GidOfName { .. } => NfsReply::Id { id: None },
                NfsOp::Mapping => NfsReply::Mapping {
                    enabled: true,
                    domain: "lab.example".into(),
                },
                _ => NfsReply::Done,
            };
            ipc::encode(&reply)
        }
        other => panic!("not an NFS request: {}", other.name()),
    });
    let NfsReply::Orders { orders } = link.nfs(NfsOp::Orders).unwrap() else {
        panic!("orders")
    };
    assert_eq!((orders[0].id, orders[0].host.as_str()), (4, "client1.lab"));
    assert_eq!(
        link.nfs(NfsOp::OrderDone {
            id: 4,
            count: 2,
            errno: 0
        })
        .unwrap(),
        NfsReply::Id { id: Some(402) }
    );
    assert_eq!(
        link.nfs(NfsOp::Failed { errno: 98 }).unwrap(),
        NfsReply::Flag { yes: true }
    );
    assert_eq!(
        link.nfs(NfsOp::NameOfUid { uid: 0 }).unwrap(),
        NfsReply::Name {
            name: Some("QSECOFR".into())
        }
    );
    assert_eq!(
        link.nfs(NfsOp::NameOfGid { gid: 7 }).unwrap(),
        NfsReply::Name { name: None }
    );
    assert_eq!(
        link.nfs(NfsOp::UidOfName {
            name: "QSYS".into()
        })
        .unwrap(),
        NfsReply::Id { id: Some(101) }
    );
    assert_eq!(
        link.nfs(NfsOp::GidOfName { name: "X".into() }).unwrap(),
        NfsReply::Id { id: None }
    );
    assert_eq!(
        link.nfs(NfsOp::Mapping).unwrap(),
        NfsReply::Mapping {
            enabled: true,
            domain: "lab.example".into()
        }
    );
}
