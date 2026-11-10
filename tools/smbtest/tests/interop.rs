// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Samba's `smbclient` and the Linux kernel CIFS client against a running
//! server. Ignored by default; run on Linux (WSL) with
//!
//! ```text
//! SMBTEST_HOST=127.0.0.1 SMBTEST_PORT=445 SMBTEST_USER=QSUSER SMBTEST_PASSWORD=... \
//! SMBTEST_SHARE=QIBM SMBTEST_SCRATCH=scratch [SMBTEST_PRINT_SHARE=PRT01] \
//! cargo test --manifest-path tools/smbtest/Cargo.toml --test interop -- --ignored --test-threads 1
//! ```
//!
//! Needs `sudo apt install smbclient cifs-utils` and `sudo modprobe cifs`
//! (the kernel tests mount, so they run as root or with `sudo -n`). These
//! cover what the smb crate cannot: byte-range locks across two connections,
//! printing, and the `srvsvc` share list over NDR 2.0.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The server and account, from the environment.
struct Env {
    host: String,
    port: String,
    user: String,
    password: String,
    share: String,
    scratch: String,
    print_share: Option<String>,
}

fn env() -> Env {
    let get = |k: &str| {
        std::env::var(k)
            .unwrap_or_else(|_| panic!("{k} is not set (see the top of tests/interop.rs)"))
    };
    Env {
        host: get("SMBTEST_HOST"),
        port: std::env::var("SMBTEST_PORT").unwrap_or_else(|_| "445".into()),
        user: get("SMBTEST_USER"),
        password: get("SMBTEST_PASSWORD"),
        share: get("SMBTEST_SHARE"),
        scratch: std::env::var("SMBTEST_SCRATCH").unwrap_or_default(),
        print_share: std::env::var("SMBTEST_PRINT_SHARE").ok(),
    }
}

fn need(program: &str, install: &str) {
    let found = Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .is_ok_and(|o| o.status.success());
    assert!(found, "{program} is missing: {install}");
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("smbtest-interop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// `smbclient` with the password in `PASSWD` (never on the command line).
fn smbclient(e: &Env, args: &[&str]) -> Output {
    need("smbclient", "sudo apt install smbclient");
    let out = Command::new("smbclient")
        .env("PASSWD", &e.password)
        .args(["-p", &e.port, "-U", &e.user])
        .args(args)
        .output()
        .expect("smbclient");
    println!(
        "smbclient {args:?}\n{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn service(e: &Env, share: &str) -> String {
    format!("//{}/{share}", e.host)
}

fn cd(e: &Env) -> String {
    if e.scratch.is_empty() {
        String::new()
    } else {
        format!("cd {};", e.scratch.replace('/', "\\"))
    }
}

#[test]
#[ignore = "needs smbclient and a running server (SMBTEST_* variables)"]
fn smbclient_lists_shares_over_srvsvc() {
    let e = env();
    let out = smbclient(&e, &["-L", &format!("//{}", e.host), "-m", "SMB3"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).to_ascii_uppercase();
    assert!(
        text.contains(&e.share.to_ascii_uppercase()),
        "{} not listed",
        e.share
    );
    if let Some(p) = &e.print_share {
        assert!(text.contains(&p.to_ascii_uppercase()), "{p} not listed");
    }
}

#[test]
#[ignore = "needs smbclient and a running server (SMBTEST_* variables)"]
fn smbclient_round_trips_on_every_dialect_signed_and_sealed() {
    let e = env();
    let dir = tmp("smbclient");
    let local = dir.join("up.bin");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();
    std::fs::write(&local, &data).unwrap();
    let cases = [
        ("SMB2_02", "sign"),
        ("SMB2_10", "sign"),
        ("SMB3_00", "encrypt"),
        ("SMB3_02", "encrypt"),
        ("SMB3_11", "sign"),
        ("SMB3_11", "encrypt"),
    ];
    for (proto, protection) in cases {
        let remote = format!("interop-{proto}-{protection}.bin");
        let back = dir.join(&remote);
        let script = format!(
            "{} put {} {remote}; get {remote} {}; rename {remote} {remote}.2; ls {remote}.2; rm {remote}.2",
            cd(&e),
            local.display(),
            back.display()
        );
        let out = smbclient(
            &e,
            &[
                &service(&e, &e.share),
                "-m",
                proto,
                &format!("--client-protection={protection}"),
                "-c",
                &script,
            ],
        );
        assert!(out.status.success(), "{proto} {protection}");
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("NT_STATUS_"),
            "{proto} {protection}: an error status"
        );
        assert_eq!(
            std::fs::read(&back).unwrap(),
            data,
            "{proto} {protection}: data differs"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "needs smbclient, a running server and SMBTEST_PRINT_SHARE"]
fn smbclient_prints_to_a_print_share() {
    let e = env();
    let Some(p) = e.print_share.clone() else {
        panic!("SMBTEST_PRINT_SHARE is not set")
    };
    let dir = tmp("print");
    let file = dir.join("page.txt");
    std::fs::write(&file, "QS/400 NetServer print share test\r\n\x0c").unwrap();
    let out = smbclient(
        &e,
        &[
            &service(&e, &p),
            "-m",
            "SMB3",
            "-c",
            &format!("print {}", file.display()),
        ],
    );
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("NT_STATUS_"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "needs smbclient and a running server (SMBTEST_* variables)"]
fn smbclient_wrong_password_is_a_logon_failure() {
    let mut e = env();
    e.password.push('x');
    let out = smbclient(&e, &[&service(&e, &e.share), "-m", "SMB3", "-c", "ls"]);
    assert!(!out.status.success());
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(all.contains("NT_STATUS_LOGON_FAILURE"), "{all}");
}

/// A kernel CIFS mount of the share, unmounted on drop.
struct Mount {
    at: PathBuf,
    sudo: bool,
}

impl Mount {
    fn new(e: &Env, tag: &str, vers: &str, creds: &Path) -> Mount {
        need("mount.cifs", "sudo apt install cifs-utils");
        let sudo = !Command::new("id")
            .arg("-u")
            .output()
            .is_ok_and(|o| o.stdout.starts_with(b"0"));
        let at = tmp(tag);
        // nosharesock: each mount is its own TCP connection and session.
        let opts = format!(
            "port={},credentials={},vers={vers},nosharesock,cache=none,uid={},gid={}",
            e.port,
            creds.display(),
            id("-u"),
            id("-g")
        );
        let mut cmd = if sudo {
            Command::new("sudo")
        } else {
            Command::new("mount")
        };
        if sudo {
            cmd.args(["-n", "mount"]);
        }
        let unc = format!("//{}/{}", e.host, e.share);
        let st = cmd
            .args(["-t", "cifs", &unc])
            .arg(&at)
            .args(["-o", &opts])
            .status()
            .expect("mount");
        assert!(
            st.success(),
            "mount {unc} failed (sudo modprobe cifs; run as root or with sudo -n)"
        );
        Mount { at, sudo }
    }

    fn path(&self, e: &Env, name: &str) -> PathBuf {
        self.at.join(&e.scratch).join(name)
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let mut cmd = if self.sudo {
            Command::new("sudo")
        } else {
            Command::new("umount")
        };
        if self.sudo {
            cmd.args(["-n", "umount"]);
        }
        let _ = cmd.arg(&self.at).status();
        let _ = std::fs::remove_dir(&self.at);
    }
}

fn id(flag: &str) -> String {
    let out = Command::new("id").arg(flag).output().expect("id");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn credentials(e: &Env) -> PathBuf {
    let dir = tmp("creds");
    let p = dir.join("credentials");
    std::fs::write(
        &p,
        format!("username={}\npassword={}\n", e.user, e.password),
    )
    .unwrap();
    p
}

#[test]
#[ignore = "needs cifs-utils, the cifs module, root (or sudo -n) and a running server"]
fn kernel_client_sees_one_mounts_writes_from_another() {
    let e = env();
    let creds = credentials(&e);
    for vers in ["2.0", "2.1", "3.0", "3.02", "3.1.1"] {
        let a = Mount::new(&e, &format!("a-{vers}"), vers, &creds);
        let b = Mount::new(&e, &format!("b-{vers}"), vers, &creds);
        let name = format!("kernel-{vers}.txt");
        std::fs::write(a.path(&e, &name), format!("written on {vers}")).unwrap();
        assert_eq!(
            std::fs::read_to_string(b.path(&e, &name)).unwrap(),
            format!("written on {vers}")
        );
        let renamed = format!("kernel-{vers}-renamed.txt");
        std::fs::rename(b.path(&e, &name), b.path(&e, &renamed)).unwrap();
        let listed: Vec<String> = std::fs::read_dir(a.at.join(&e.scratch))
            .unwrap()
            .map(|d| d.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(listed.contains(&renamed), "{vers}: {listed:?}");
        std::fs::remove_file(a.path(&e, &renamed)).unwrap();
        assert!(!b.path(&e, &renamed).exists());
    }
    let _ = std::fs::remove_dir_all(creds.parent().unwrap());
}

#[test]
#[ignore = "needs cifs-utils, the cifs module, root (or sudo -n) and a running server"]
fn kernel_client_lock_conflicts_across_two_connections() {
    let e = env();
    let creds = credentials(&e);
    let a = Mount::new(&e, "lock-a", "3.1.1", &creds);
    let b = Mount::new(&e, "lock-b", "3.1.1", &creds);
    std::fs::write(a.path(&e, "locked.bin"), [0u8; 4096]).unwrap();
    // flock(2) on cifs is an SMB2 LOCK over the whole file.
    let fa = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(a.path(&e, "locked.bin"))
        .unwrap();
    let fb = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(b.path(&e, "locked.bin"))
        .unwrap();
    fa.try_lock().expect("first lock");
    assert!(
        fb.try_lock().is_err(),
        "the second connection took a conflicting lock"
    );
    fa.unlock().unwrap();
    fb.try_lock().expect("lock after the first is released");
    fb.unlock().unwrap();
    drop((fa, fb));
    std::fs::remove_file(a.path(&e, "locked.bin")).unwrap();
    let _ = std::fs::remove_dir_all(creds.parent().unwrap());
}
