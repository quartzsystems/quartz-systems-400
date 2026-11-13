// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A remote mount over the link: `RemoteIfs` whose far end answers
//! `ifs` requests as the storage service does, running a request that
//! waits again with the same token until it is done (as the loop parks
//! it), over an `Ifs<MemBackend>` with a remote file system in memory.
//! `MOUNT TYPE(*NFS)`, the operations, the mount table, block I/O
//! helpers' work and `UNMOUNT` cross the wire codec both ways.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ifs::Errno;
use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::lease::defer::deferring;
use ifs::ops::audit::MemAudit;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_WRONLY};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner};
use ifs::remote::mem::{MemFactory, MemServer};
use ifs::remote::{CcsidSel, HelperResult, MountCcsid, RemoteConfig, RemoteInfo, request};
use ifs::vfs::qsys::MemQsys;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;
use stg::ifs::serve;
use stg::remote::RemoteIfs;
use stg::{Link, Request};

struct Far {
    ifs: Arc<Ifs<MemBackend>>,
    env: MemEnv,
    audit: Arc<MemAudit>,
    server: Arc<MemServer>,
}

fn far() -> Far {
    let env = MemEnv::default()
        .user("QSYS", 2)
        .user("QSECOFR", 0)
        .user("BOB", 100);
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(MemQsys::new(Auth::USE)),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    let server = Arc::new(MemServer::new());
    let f = Arc::new(MemFactory::new("*NFS"));
    f.add(
        "srv:/exp",
        Arc::clone(&server),
        RemoteInfo {
            name: "NFS".into(),
            ..RemoteInfo::default()
        },
    );
    ifs.set_remote_factory(Some(f), RemoteConfig::default());
    Far {
        ifs: Arc::new(ifs),
        env,
        audit: Arc::new(MemAudit::default()),
        server,
    }
}

fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::IOSYSCFG.0;
    }
    p
}

/// A job's connection: each request runs, and runs again with its token
/// after what it waits for is done, as the service parks it.
fn connect(far: &Far, user: &str, owner: u64) -> RemoteIfs {
    let ifs = Arc::clone(&far.ifs);
    let env = far.env.clone();
    let audit = Arc::clone(&far.audit);
    let p = principal(user);
    let link = Link::answering(move |req| match req {
        Request::IfsReq { op } => {
            let mut token = None;
            loop {
                if let Some(t) = token {
                    request::rerun(t);
                }
                let (reply, d) = deferring(true, || {
                    let cx = Ctx {
                        owner: Owner(owner),
                        principal: &p,
                        env: &env,
                        audit: &*audit,
                        job_ccsid: 37,
                    };
                    let direct = ifs::client::Direct { ifs: &ifs, cx: &cx };
                    serve(&direct, op.clone())
                });
                let Some(d) = d else {
                    break ipc::encode(&reply);
                };
                token = request::last();
                ifs.wait_deferral(&d);
            }
        }
        other => panic!("not an IFS request: {}", other.name()),
    });
    RemoteIfs::new(Rc::new(RefCell::new(link)))
}

#[test]
fn a_remote_mount_over_the_link() {
    let far = far();
    let sec = connect(&far, "QSECOFR", 1);
    let bob = connect(&far, "BOB", 2);
    sec.mkdir("/nfs", 0o777).unwrap();
    let cc = MountCcsid {
        data: CcsidSel::Ccsid(819),
        path: CcsidSel::Ascii,
    };
    assert_eq!(
        bob.mount_remote("*NFS", "srv:/exp", "/nfs", "", cc),
        Err(Errno::EPERM)
    );
    let id = sec
        .mount_remote("*NFS", "srv:/exp", "/nfs", "rw", cc)
        .unwrap();
    assert!(id >= 100);
    let rows = sec.mounts().unwrap();
    let row = rows.iter().find(|m| m.id == id).unwrap();
    assert_eq!(
        (row.kind.as_str(), row.at.as_str(), row.source.as_str()),
        ("*NFS", "/nfs", "srv:/exp")
    );
    assert_eq!((row.data_ccsid, row.responding), (819, Some(true)));
    bob.mkdir("/nfs/bob", 0o755).unwrap();
    let fd = bob
        .open(
            "/nfs/bob/a.txt",
            OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644),
        )
        .unwrap();
    let big = vec![0x5au8; ifs::client::CHUNK + 10];
    write_all(&bob, fd, &big).unwrap();
    bob.close(fd).unwrap();
    assert_eq!(far.server.file("bob/a.txt").unwrap(), big);
    let st = bob.stat("/nfs/bob/a.txt", true).unwrap();
    assert_eq!((st.size, st.uid, st.ccsid), (big.len() as u64, 100, 819));
    let fd = bob.open("/nfs/bob/a.txt", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(read_all(&bob, fd).unwrap(), big);
    bob.close(fd).unwrap();
    assert_eq!(bob.statvfs("/nfs").unwrap().name, "NFS");
    // A helper finds nothing to do at rest, and a stray answer is refused.
    assert_eq!(bob.remote_take(), Err(Errno::EPERM));
    assert_eq!(sec.remote_take().unwrap(), None);
    assert_eq!(
        sec.remote_post(id, 99, &HelperResult::default()),
        Err(Errno::ENOENT)
    );
    assert_eq!(sec.unmount_kind("*NFS", "/nfs"), Ok(1));
    assert!(far.server.unmounted());
    assert_eq!(bob.stat("/nfs/bob", true), Err(Errno::ENOENT));
}
