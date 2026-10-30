// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PASE mechanism for real, as root on Linux: `pase-probe` (built
//! for the musl target; `PASE_PROBE` names it) runs from a fake IFS's
//! `/QOpenSys/tmp` and reports what it sees. `tests/linux-root.sh` runs
//! this under WSL; without `PASE_PROBE` the test does nothing.

#![cfg(target_os = "linux")]

use std::io::Read;
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use pase::{Exit, Launch};

fn tree() -> PathBuf {
    let base = std::env::temp_dir().join(format!("pase-ifs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for d in ["root/home/QSTEST", "QOpenSys/tmp", "QOpenSys/usr/bin", "QOpenSys/etc", "stage"] {
        std::fs::create_dir_all(base.join(d)).unwrap();
    }
    std::os::unix::fs::symlink("usr/bin", base.join("QOpenSys/bin")).unwrap();
    std::fs::write(base.join("QOpenSys/etc/profile"), b"# profile\n").unwrap();
    base
}

#[test]
fn a_probe_runs_in_its_own_namespaces() {
    let Some(probe) = std::env::var_os("PASE_PROBE") else {
        eprintln!("PASE_PROBE not set: skipped");
        return;
    };
    let base = tree();
    let target = base.join("QOpenSys/tmp/paseprobe");
    std::fs::copy(&probe, &target).unwrap();
    std::fs::set_permissions(&target, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let (mut r, w) = std::io::pipe().unwrap();
    let null = std::fs::File::open("/dev/null").unwrap();
    let mut l = Launch::new(
        "/QOpenSys/tmp/paseprobe",
        1234,
        1235,
        [null.as_raw_fd(), w.as_raw_fd(), w.as_raw_fd()],
    );
    l.ifs = base.clone();
    l.qopensys = None;
    l.stage = base.join("stage");
    l.argv = vec!["paseprobe".into(), "one".into(), "two".into()];
    l.env = vec![("PATH".into(), "/QOpenSys/usr/bin:/usr/bin".into())];
    l.cwd = "/home/QSTEST".into();
    let mut child = pase::start(&l).expect("started");
    drop(w);
    let mut out = String::new();
    r.read_to_string(&mut out).unwrap();
    let end = child.wait().unwrap();
    println!("{out}");
    assert_eq!(end, Exit::Code(0), "{out}");
    assert!(out.contains("uid=1234 gid=1235"), "{out}");
    assert!(out.contains("pid=1\n"), "{out}");
    assert!(out.contains("procs=1\n"), "the host's processes show:\n{out}");
    assert!(out.contains("root: QOpenSys bin dev etc home proc tmp usr"), "{out}");
    assert!(out.contains("tmp: paseprobe"), "{out}");
    assert!(out.contains("home=true"), "{out}");
    assert!(out.contains("pidfd_open: refused errno 1"), "{out}");
    assert!(out.contains("args: one two"), "{out}");
    // A program outside the view does not start.
    let mut l2 = l.clone();
    l2.program = "/QSYS.LIB/X.PGM".into();
    assert!(matches!(pase::start(&l2), Err(pase::PaseError::NotVisible(_))));
    let _ = std::fs::remove_dir_all(&base);
}
