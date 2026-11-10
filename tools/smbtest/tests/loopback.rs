// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every scenario against the vendored server (`net/smb-server`) in this
//! process on loopback, over a directory, with a logon verifier as QS/400's
//! `QZLSFILE` installs one. The client and the server have different authors.

use std::net::SocketAddr;
use std::path::PathBuf;

use hmac::{Hmac, Mac};
use md4::{Digest, Md4};
use smb_server::{
    Access, LocalFsBackend, Logon, LogonVerifier, Share, SmbServer, SmbServerBuilder,
};
use smbtest::{Outcome, Report, Target, scenarios};

const USER: &str = "QSUSER";
const PASSWORD: &str = "Secret-400";

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 16] {
    let mut m = Hmac::<md5::Md5>::new_from_slice(key).unwrap();
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

/// One profile, any case, `NTLMv2` checked here as the storage service would.
struct OneProfile;

impl LogonVerifier for OneProfile {
    fn verify(&self, user: &str, domain: &str, challenge: &[u8; 8], resp: &[u8]) -> Option<Logon> {
        if !user.eq_ignore_ascii_case(USER) || resp.len() < 16 {
            return None;
        }
        let nt: [u8; 16] = Md4::digest(utf16(PASSWORD)).into();
        let key = hmac(&nt, &[&utf16(&format!("{}{domain}", user.to_uppercase()))]);
        let (proof, blob) = resp.split_at(16);
        let computed = hmac(&key, &[challenge, blob]);
        (proof == computed).then(|| Logon {
            user: USER.into(),
            session_base_key: hmac(&key, &[&computed]),
            guest: false,
        })
    }
}

/// A fresh directory under the system temp directory, removed on drop.
struct TempRoot(PathBuf);

impl TempRoot {
    fn new(tag: &str) -> TempRoot {
        let p = std::env::temp_dir().join(format!("smbtest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("scratch")).unwrap();
        TempRoot(p)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn server(root: &TempRoot) -> SmbServerBuilder {
    SmbServer::builder()
        .logon_verifier(OneProfile)
        .require_signing(true)
        .share(
            Share::new("DATA", LocalFsBackend::new(&root.0).unwrap()).any_user(Access::ReadWrite),
        )
}

async fn start(builder: SmbServerBuilder) -> SocketAddr {
    let server = builder
        .listen("127.0.0.1:0".parse().unwrap())
        .build()
        .expect("build");
    let addr = server.bind().await.expect("bind");
    tokio::spawn(async move {
        let _ = server.serve().await;
    });
    addr
}

fn target(addr: SocketAddr) -> Target {
    let mut t = Target::new(
        "127.0.0.1",
        addr.port(),
        "qsuser",
        PASSWORD,
        "DATA",
        "scratch",
    );
    t.copy_bytes = 16 << 20;
    t.large_bytes = 4 << 20;
    t
}

fn ok(r: &Report, name: &str) {
    assert!(
        matches!(r.get(name), Some(Outcome::Ok(_))),
        "{name}: {:?}\n{r}",
        r.get(name)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_scenario_passes_against_the_vendored_server() {
    let root = TempRoot::new("all");
    let addr = start(server(&root)).await;
    let r = scenarios::run_all(&target(addr)).await;
    println!("{r}");
    assert!(r.failures().is_empty(), "failures: {:?}\n{r}", r.failures());
    for name in scenarios::names() {
        assert!(r.get(&name).is_some(), "{name} did not run\n{r}");
    }
    for (name, _) in scenarios::MISSING {
        assert!(matches!(r.get(name), Some(Outcome::Skip(_))));
    }
    // The client binds srvsvc with NDR64 only; this server speaks NDR 2.0.
    assert!(matches!(r.get("share-enum"), Some(Outcome::Skip(_))), "{r}");
    assert_eq!(r.detail("cleanup", "left over: "), Some("none"), "{r}");
    assert_eq!(r.detail("negotiate-2.0.2", "signing: "), Some("HmacSha256"));
    assert_eq!(r.detail("negotiate-3.0", "signing: "), Some("AesCmac"));
    assert!(r.detail("copy", "in: ").is_some_and(|s| s.contains("MB/s")));
    // The run directory is gone; the scratch directory is empty again.
    assert_eq!(
        std::fs::read_dir(root.0.join("scratch")).unwrap().count(),
        0
    );
}

/// Matches Windows' own server (`oracle/netserver/windows-negotiate.txt`):
/// every dialect, and 3.1.1 with no ciphers refused as an invalid parameter.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn negotiate_only_probe_answers_as_windows_does() {
    let root = TempRoot::new("probe");
    let addr = start(server(&root)).await;
    let r = scenarios::probe::run(&target(addr)).await;
    assert_eq!(r.failures(), ["probe-3.1.1-noenc"], "{r}");
    assert!(
        matches!(r.get("probe-3.1.1-noenc"), Some(Outcome::Fail(m)) if m.contains("0xC000000D")),
        "{r}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn encryption_required_by_the_server_refuses_smb2() {
    let root = TempRoot::new("enc");
    let addr = start(server(&root).encrypt_data(true)).await;
    let mut t = target(addr);
    t.only = [
        "negotiate-2.1",
        "negotiate-3.1.1",
        "scratch-dir",
        "encrypt-3.0",
        "encrypt-3.1.1",
        "cleanup",
    ]
    .map(str::to_string)
    .to_vec();
    let r = scenarios::run_all(&t).await;
    println!("{r}");
    assert!(
        matches!(r.get("negotiate-2.1"), Some(Outcome::Fail(_))),
        "{r}"
    );
    for name in [
        "negotiate-3.1.1",
        "scratch-dir",
        "encrypt-3.0",
        "encrypt-3.1.1",
        "cleanup",
    ] {
        ok(&r, name);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bad_signons_are_logon_failures() {
    let root = TempRoot::new("bad");
    let addr = start(server(&root)).await;
    let mut t = target(addr);
    t.only = [
        "signon-good",
        "signon-wrong-password",
        "signon-unknown-user",
    ]
    .map(str::to_string)
    .to_vec();
    let r = scenarios::run_all(&t).await;
    for name in &t.only {
        ok(&r, name);
    }
    t.password = "nope".into();
    t.only = vec!["signon-good".into()];
    let r = scenarios::run_all(&t).await;
    assert!(
        matches!(r.get("signon-good"), Some(Outcome::Fail(m)) if m.contains("C000006D")),
        "{r}"
    );
}
