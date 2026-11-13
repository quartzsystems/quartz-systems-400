// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a file server job does over its link: calls as the users its
//! clients are (one request each, `IfsOp::As`), resolved from the
//! store's profiles and kept until a profile changes, audited as the
//! user; handles to objects; locks of its clients that wait in the
//! service and are told through its poll.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ifs::Errno;
use ifs::backend::mem::MemBackend;
use ifs::client::{Caller, IfsClient};
use ifs::lock::range::RangeKind;
use ifs::ops::audit::FileId;
use ifs::ops::flags::{F_SETLK, O_CREAT, O_RDWR, O_WRONLY, SEEK_SET};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner};
use qsyslib::env::EnvCache;
use qsyslib::{JournalAudit, MemAccess, StoreAccess, StoreEnv, StoreQsys};
use sec::auth::principal::Principal;
use sec::profile::payload::UsrPrf;
use sec::profile::spcaut::SpecialAuth;
use stg::ifs::{IfsOp, serve};
use stg::remote::RemoteIfs;
use stg::{Link, Request};
use store::obj::header::Authority;
use store::obj::{JobHandle, ObjectId, ops};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

/// The store's access with what the service keeps across requests.
struct Kept {
    mem: MemAccess,
    cache: EnvCache,
}

impl StoreAccess for Kept {
    fn with_main<T>(
        &self,
        f: &mut dyn FnMut(&mut Txn<'_>, ObjectId) -> Result<T, Errno>,
    ) -> Result<T, Errno> {
        self.mem.with_main(f)
    }
    fn job(&self) -> JobHandle {
        self.mem.job()
    }
    fn sysval(&self, name: &str) -> Option<String> {
        self.mem.sysval(name)
    }
    fn now(&self) -> u64 {
        self.mem.now()
    }
    fn env_cache(&self) -> Option<&EnvCache> {
        Some(&self.cache)
    }
}

fn profile(t: &mut Txn<'_>, qsys: ObjectId, user: &str, uid: u32, groups: &[&str]) {
    let mut p = UsrPrf::new(user, 40);
    p.uid = uid;
    if user == "QSECOFR" {
        p.spcaut = SpecialAuth::ALL;
    }
    if user == "DEV" {
        p.gid = 500;
    }
    if let Some(g) = groups.first() {
        p.grpprf = (*g).into();
    }
    sec::profile::objects::create(t, qsys, &p, "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
}

fn access() -> Arc<Kept> {
    let d = Arc::new(MemBlockDev::with_seed(16384, 11));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    profile(&mut t, qsys, "QSYS", 101, &[]);
    profile(&mut t, qsys, "QSECOFR", 0, &[]);
    profile(&mut t, qsys, "DEV", 2000, &[]);
    profile(&mut t, qsys, "BOB", 1000, &[]);
    profile(&mut t, qsys, "ANN", 1001, &["DEV"]);
    profile(&mut t, qsys, "QNFSANON", 4_294_770_710, &[]);
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        qsys,
        qsname::Name::parse("AUDRCV0001").unwrap(),
        &jrn::rcv::CrtJrnRcvOpts {
            now: NOW,
            ..jrn::rcv::CrtJrnRcvOpts::default()
        },
    )
    .unwrap();
    jrn::journal::crtjrn(
        &mut t,
        qsys,
        qsname::Name::parse("QAUDJRN").unwrap(),
        rcv,
        &jrn::CrtJrnOpts {
            now: NOW,
            ..jrn::CrtJrnOpts::default()
        },
    )
    .unwrap();
    t.commit().unwrap();
    Arc::new(Kept {
        mem: MemAccess::new(store, qsys),
        cache: EnvCache::default(),
    })
}

struct Far {
    access: Arc<Kept>,
    ifs: Arc<Ifs<MemBackend>>,
}

fn far() -> Far {
    let access = access();
    let ifs = Ifs::new(
        MemBackend::new(),
        Box::new(StoreQsys::new(Arc::clone(&access))),
        Config::default(),
    );
    ifs.ensure_layout(&StoreEnv::new(&*access)).unwrap();
    Far {
        access,
        ifs: Arc::new(ifs),
    }
}

/// The server job's connection (running as `QSECOFR`), served as the
/// service serves it: audit entries charged to the user each event
/// names, as the service's flush charges them.
fn connect(far: &Far, owner: u64) -> RemoteIfs {
    let ifs = Arc::clone(&far.ifs);
    let access = Arc::clone(&far.access);
    let link = Link::answering(move |req| {
        let Request::IfsReq { op } = req else {
            panic!("not an IFS request: {}", req.name());
        };
        let env = StoreEnv::new(&*access);
        let p = env.principal_of_store("QSECOFR");
        let cx = Ctx {
            owner: Owner(owner),
            principal: &p,
            env: &env,
            audit: &JournalAudit,
            job_ccsid: 37,
        };
        let reply = serve(&ifs::client::Direct { ifs: &ifs, cx: &cx }, op);
        let events: Vec<_> = sec::audit::take()
            .into_iter()
            .map(|e| {
                let user = e.user.clone().unwrap_or_else(|| "QSECOFR".into());
                let job = jrn::JobInfo {
                    name: "QNFSNFSD".into(),
                    user,
                    number: u32::try_from(owner).unwrap(),
                };
                (e, job)
            })
            .collect();
        access
            .with_main(&mut |t, qsys| {
                sec::audit::write::write(t, qsys, &events).map_err(|_| Errno::EIO)
            })
            .unwrap();
        ipc::encode(&reply)
    });
    RemoteIfs::new(Rc::new(RefCell::new(link)))
}

/// The principal of a store profile.
trait Load {
    fn principal_of_store(&self, name: &str) -> Principal;
}

impl<S: StoreAccess> Load for StoreEnv<'_, S> {
    fn principal_of_store(&self, name: &str) -> Principal {
        ifs::auth::AuthEnv::principal_of(self, name).unwrap()
    }
}

#[test]
fn a_server_calls_as_its_clients_users() {
    let far = far();
    let job = connect(&far, 9);
    job.mkdir("/home/bob", 0o750).unwrap();
    job.chgown("/home/bob", "BOB", true).unwrap();
    job.chgpgp("/home/bob", Some("DEV"), true).unwrap();
    let bob = job.as_user(Caller::Uid { uid: 1000 });
    let ann = job.as_user(Caller::Uid { uid: 1001 });
    let fd = bob
        .open("/home/bob/f", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o640))
        .unwrap();
    bob.close(fd).unwrap();
    assert_eq!(bob.stat("/home/bob/f", true).unwrap().owner, "BOB");
    // ANN reads through her group profile, from the store.
    assert_eq!(ann.read_dir("/home/bob").unwrap().len(), 1);
    assert_eq!(ann.mkdir("/home/bob/x", 0o755), Err(Errno::EACCES));
    // Unknown users and the anonymous one.
    let nobody = job.as_user(Caller::Uid { uid: 4242 });
    assert_eq!(nobody.stat("/home", true), Err(Errno::ENOTENROLL));
    assert_eq!(nobody.read_dir("/home"), Err(Errno::ENOTENROLL));
    // The refusal is ANN's in the audit journal.
    let back = far
        .access
        .with_main(&mut |t, qsys| {
            sec::audit::write::entries(t, qsys, &jrn::Filter::default()).map_err(|_| Errno::EIO)
        })
        .unwrap();
    assert!(
        back.iter()
            .any(|(e, _)| &e.kind == b"AF" && e.job.user == "ANN"),
        "{back:?}"
    );
    // The profiles were indexed once, and again after one changed.
    let builds = far.access.cache.ids.builds();
    ann.stat("/home/bob/f", true).unwrap();
    assert_eq!(far.access.cache.ids.builds(), builds);
    far.access
        .with_main(&mut |t, qsys| {
            let (id, mut p) = sec::profile::objects::read(t, qsys, "BOB").unwrap();
            p.text = "changed".into();
            sec::profile::objects::write(t, id, &p).map_err(|_| Errno::EIO)
        })
        .unwrap();
    bob.stat("/home/bob/f", true).unwrap();
    assert_eq!(far.access.cache.ids.builds(), builds + 1);
    // Nesting is refused.
    let twice = IfsOp::As {
        who: Caller::Anonymous,
        op: Box::new(IfsOp::As {
            who: Caller::Anonymous,
            op: Box::new(IfsOp::Getcwd),
        }),
    };
    assert_eq!(
        stg::ifs::serve(&ifs::client::Unavailable(Errno::EIO), twice),
        stg::ifs::IfsReply::Text {
            r: Err(Errno::EINVAL)
        }
    );
}

#[test]
fn handles_and_waiting_locks_over_the_link() {
    let far = far();
    let job = connect(&far, 9);
    let other = connect(&far, 10);
    job.mkdir("/home/d", 0o777).unwrap();
    job.chmod("/home/d", 0o777).unwrap();
    // The service maps Caller::Anonymous to QNFSANON (slic's ifscaller).
    let anon = job.as_user(Caller::Profile {
        name: "QNFSANON".into(),
    });
    let fd = anon
        .open("/home/d/f", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    let st = anon.fstat(fd).unwrap();
    assert_eq!(st.owner, "QNFSANON");
    let id = FileId {
        fs: st.vfs,
        ino: st.ino,
        generation: st.ino_gen,
    };
    far.ifs.forget_ids();
    let (p, s) = anon.stat_by_id(id).unwrap();
    assert_eq!((p.as_str(), s.ino), ("/home/d/f", id.ino));
    let (fd2, _) = job.open_by_id(id, OpenArgs::new(O_RDWR)).unwrap();
    // The other job locks it; the server's client waits in the service.
    let ofd = other.open("/home/d/f", OpenArgs::new(O_RDWR)).unwrap();
    other
        .lock_range(ofd, F_SETLK, RangeKind::Write, SEEK_SET, 0, 0)
        .unwrap();
    let t = job
        .lock_wait(fd2, 77, RangeKind::Write, SEEK_SET, 0, 10)
        .unwrap()
        .unwrap();
    assert!(job.lease_orders(0).unwrap().locks.is_empty());
    other.close(ofd).unwrap();
    assert_eq!(job.lease_orders(0).unwrap().locks, vec![t]);
    // Held by sub-owner 77: the job's own lock conflicts.
    assert_eq!(
        job.lock_range_as(fd, 0, F_SETLK, RangeKind::Write, SEEK_SET, 0, 1),
        Err(Errno::EAGAIN)
    );
    assert_eq!(job.lock_release_sub(77), Ok(1));
    assert!(!job.lock_cancel(t).unwrap());
    unlink_stale(&job, id);
}

fn unlink_stale(job: &RemoteIfs, id: FileId) {
    job.unlink("/home/d/f").unwrap();
    assert_eq!(job.stat_by_id(id), Err(Errno::ESTALE));
}
