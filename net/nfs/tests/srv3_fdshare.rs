// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A thread's kept write descriptor does not hold a share reservation
//! against the job's other threads: an NFSv3 `WRITE` on one thread (its
//! connection then idle), and an NLM `SHARE` denying writes on another
//! thread of the same daemon job is granted at once.

#![allow(clippy::doc_markdown, clippy::many_single_char_names)]

mod srv3_common;

use std::sync::Arc;

use nfs::nlm::Nlm;
use nfs::nlm::state::{NlmConfig, NlmState};
use nfs::rpc::{ClientConfig, Dispatcher};
use nfs::server::SystemClock;
use nfs::server::locate::FixedLocator;
use nfs::server::run;
use nfs::types::nfs3 as t3;
use nfs::types::nlm::NLM_PROG::NLM_VERSX;
use nfs::types::nlm::{fsh_access, fsh_mode, netobj, nlm_share, nlm_shareargs, nlm_stats};
use srv3_common::{Client, V3, World};

#[test]
fn an_idle_threads_write_descriptor_does_not_refuse_a_share() {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    w.write_file("/home/exp/f", b"0123456789");
    {
        use ifs::client::IfsClient;
        w.job(9).chmod("/home/exp/f", 0o666).unwrap();
    }
    let fh = w.handle(1, "/home/exp/f");
    // Thread A: NFS version 3, writing (its descriptor is kept).
    let p3 = w.nfs3();
    let c3 = Client::new("10.0.0.5:900", 100);
    V3 { p: &p3, c: &c3 }
        .write(&fh, 0, b"x", t3::stable_how::FILE_SYNC)
        .unwrap();
    assert_eq!(p3.vfs().cached_fds(), 1);
    // Thread B of the same job: the lock manager.
    let st = NlmState::new(NlmConfig {
        monitor: false,
        locator: Arc::new(FixedLocator::new()),
        rpc: ClientConfig::default(),
        grace: std::time::Duration::ZERO,
        grant_tries: 1,
        clock: Arc::new(SystemClock),
    });
    let p = Nlm::new(w.vfs(), w.gate(), st);
    let mut c = Client::new("10.0.0.6:900", 100);
    c.d = Dispatcher::new();
    c.d.set_cache(run::nlm_cache());
    let a = NLM_VERSX::Args::NLM_SHARE(nlm_shareargs {
        cookie: netobj(b"s".to_vec()),
        share: nlm_share {
            caller_name: b"pc".to_vec(),
            fh: netobj(fh.clone()),
            oh: netobj(b"pc".to_vec()),
            mode: fsh_mode::fsm_DW,
            access: fsh_access::fsa_R,
        },
        reclaim: false,
    });
    match c.call::<_, NLM_VERSX::Res>(&p, &a) {
        NLM_VERSX::Res::NLM_SHARE(r) => assert_eq!(r.stat, nlm_stats::nlm_granted),
        r => panic!("{r:?}"),
    }
    // Thread A's descriptor went; a write through it again is refused
    // by the share, as it must be.
    assert_eq!(p3.vfs().cached_fds(), 0);
    assert_eq!(
        V3 { p: &p3, c: &c3 }
            .write(&fh, 0, b"y", t3::stable_how::FILE_SYNC)
            .unwrap_err(),
        t3::nfsstat3::NFS3ERR_ACCES
    );
}
