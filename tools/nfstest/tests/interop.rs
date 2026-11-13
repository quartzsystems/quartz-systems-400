// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! nfstest against a second server, and a second client against QS/400's
//! server; all ignored, for a Linux host with what they need
//! (`tools/nfstest/README.md`, "Interop"):
//!
//! - `NFSTEST_LINUX=host:/export` (and `NFSTEST_LINUX_SCRATCH`, default
//!   `nfstest`; `NFSTEST_UID`/`NFSTEST_GID`, default 0) names a Linux
//!   `nfsd` export the scenarios may write in: every v3, NLM and NSM
//!   scenario, then 4.0 and 4.1, against it validates nfstest itself.
//! - `kernel_client_mounts_the_server` needs `mount.nfs` and root: the
//!   Linux client mounts QS/400's server on loopback (explicit ports, no
//!   lock manager), writes, reads back, lists and unmounts.

mod common;

fn run(args: &[String]) -> (i32, String) {
    let mut out = Vec::new();
    let code = nfstest::cli::run(args, &mut out);
    (code, String::from_utf8(out).unwrap())
}

/// `host`, `export` from `NFSTEST_LINUX`.
fn linux() -> (String, String) {
    let v = std::env::var("NFSTEST_LINUX").expect("NFSTEST_LINUX=host:/export");
    let (h, e) = v.split_once(':').expect("host:/export");
    (h.to_string(), e.to_string())
}

fn linux_args(vers: &str) -> Vec<String> {
    let (host, export) = linux();
    let scratch = std::env::var("NFSTEST_LINUX_SCRATCH").unwrap_or_else(|_| "nfstest".into());
    let uid = std::env::var("NFSTEST_UID").unwrap_or_else(|_| "0".into());
    let gid = std::env::var("NFSTEST_GID").unwrap_or_else(|_| "0".into());
    [
        host.as_str(),
        export.as_str(),
        scratch.as_str(),
        "--vers",
        vers,
        "--uid",
        &uid,
        "--gid",
        &gid,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

#[test]
#[ignore = "needs NFSTEST_LINUX=host:/export, a Linux nfsd export"]
fn every_v3_scenario_against_linux() {
    let (code, out) = run(&linux_args("3"));
    println!("{out}");
    assert_eq!(code, 0, "{out}");
}

#[test]
#[ignore = "needs NFSTEST_LINUX=host:/export, a Linux nfsd export"]
fn every_4_0_scenario_against_linux() {
    let (code, out) = run(&linux_args("4.0"));
    println!("{out}");
    assert_eq!(code, 0, "{out}");
}

#[test]
#[ignore = "needs NFSTEST_LINUX=host:/export, a Linux nfsd export"]
fn every_4_1_scenario_against_linux() {
    let (code, out) = run(&linux_args("4.1"));
    println!("{out}");
    assert_eq!(code, 0, "{out}");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs mount.nfs and root"]
fn kernel_client_mounts_the_server() {
    use std::process::Command;

    use nfstest::ctx::{Ctx, dump, prog};
    use nfstest::target::Target;

    let s = common::start(&common::Options::default());
    let mut t = Target::new("127.0.0.1", common::EXPORT, "");
    t.pmap_port = s.pmap_port;
    let c = Ctx::resolve(&t).unwrap();
    let list = dump(&c).unwrap();
    let port = |p: u32| {
        list.iter()
            .find(|m| m.prog == p && m.vers == 3 && m.prot == 6)
            .map(|m| m.port)
            .unwrap()
    };
    let dir = std::env::temp_dir().join(format!("nfstest-mnt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let opts = format!(
        "vers=3,proto=tcp,port={},mountport={},mountproto=tcp,nolock,soft,timeo=50",
        port(prog::NFS),
        port(prog::MOUNT)
    );
    let st = Command::new("mount")
        .args(["-t", "nfs", "-o", &opts])
        .arg(format!("127.0.0.1:{}", common::EXPORT))
        .arg(&dir)
        .status()
        .unwrap();
    assert!(st.success(), "mount failed");
    let f = dir.join("from-the-kernel");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let r = (|| -> std::io::Result<()> {
        std::fs::write(&f, &data)?;
        assert_eq!(std::fs::read(&f)?, data);
        let names: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert!(names.iter().any(|n| n == "from-the-kernel"));
        std::fs::remove_file(&f)
    })();
    let _ = Command::new("umount").arg(&dir).status();
    let _ = std::fs::remove_dir(&dir);
    r.unwrap();
}
