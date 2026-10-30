// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server end to end over an in-memory stream: first start, key
//! exchange with every method and host key, password and key sign-on,
//! the access rules, `MaxAuthTries` and `QMAXSIGN`, exec and a terminal
//! shell through QSH.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{PASSWORD, connect, connect_with, signed_on, world};
use sshd::SshError;
use sshd::client::{Incoming, Options};
use sshd::config::paths::{CONFIG_FILE, ETC_DIR};
use sshd::keys::{KeyKind, PrivateKey, PublicKey};
use sshd::transport::algo;

#[test]
fn first_start_writes_config_and_keys_once() {
    let w = common::world_with("", |d| d);
    // An empty file is kept as written; a missing one gets the default.
    assert_eq!(w.fs.get(CONFIG_FILE).unwrap(), b"");
    let mut fps = Vec::new();
    for k in KeyKind::ALL {
        let path = format!("{ETC_DIR}/{}", k.file_name());
        let pem = String::from_utf8(w.fs.get(&path).unwrap()).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
        let pubtext = String::from_utf8(w.fs.get(&format!("{path}.pub")).unwrap()).unwrap();
        let key = PrivateKey::from_pem(&pem).unwrap().public();
        assert!(pubtext.starts_with(&key.openssh()));
        fps.push(key.fingerprint());
    }
    // A second start finds them and does not make new ones.
    let again = common::start(&w.fs, &w.accounts, |d| d);
    let fps2: Vec<String> = again.keys.0.iter().map(|k| k.public().fingerprint()).collect();
    assert_eq!(fps, fps2);

    // No configuration at all: the default one is written.
    let fs = w.fs.clone();
    let c = fs.client("QSECOFR");
    ifs::client::IfsClient::unlink(&c, CONFIG_FILE).unwrap();
    c.release();
    let s = common::start(&w.fs, &w.accounts, |d| d);
    let text = String::from_utf8(w.fs.get(CONFIG_FILE).unwrap()).unwrap();
    assert!(text.contains("Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server"));
    assert_eq!(s.config.subsystem("sftp"), Some(sshd::config::paths::SFTP_SERVER));
}

#[test]
fn every_kex_method_and_host_key() {
    let w = world("");
    for kex in [algo::MLKEM768X25519, algo::CURVE25519, algo::CURVE25519_LIBSSH, algo::ECDH_P256] {
        for hk in ["ssh-ed25519", "ecdsa-sha2-nistp256", "rsa-sha2-512", "rsa-sha2-256"] {
            let opts = Options {
                kex: vec![kex.into()],
                host_key: vec![hk.into()],
                ..Options::default()
            };
            let mut c = connect_with(&w.server, &opts);
            assert!(c.host_key.signs_with(hk), "{kex} {hk}");
            assert!(c.server_ident.starts_with("SSH-2.0-QS400_V"));
            assert!(c.server_sig_algs.iter().any(|a| a == "rsa-sha2-256"));
            assert!(c.auth_password("qstest", PASSWORD).unwrap(), "{kex} {hk}");
            let out = c.run("echo ok", b"").unwrap();
            assert_eq!(out.stdout, b"ok\n");
        }
    }
}

#[test]
fn password_sign_on_and_exec() {
    let w = world("");
    let mut c = connect(&w.server);
    assert_eq!(c.auth_none("qstest").unwrap(), ["publickey", "password"]);
    assert!(!c.auth_password("qstest", "wrong").unwrap());
    assert!(c.auth_password("qstest", PASSWORD).unwrap());
    let out = c.run("echo hello; echo oops >&2; exit 3", b"").unwrap();
    assert_eq!(out.stdout, b"hello\n");
    assert_eq!(out.stderr, b"oops\n");
    assert_eq!(out.status, Some(3));
    let out = c.run("read x; echo got $x; echo $USER $HOME", b"abc\n").unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "got abc\nQSTEST /home/QSTEST\n");
    let out = c.run("echo $SSH_CONNECTION", b"").unwrap();
    assert_eq!(out.stdout, b"10.1.2.3 50000 10.0.0.1 22\n");
    // A shell without a terminal reads its commands from the input.
    let ch = c.session().unwrap();
    assert!(c.shell(ch).unwrap());
    c.send_data(ch, b"echo one\nexit 4\n").unwrap();
    c.send_eof(ch).unwrap();
    let out = c.collect(ch).unwrap();
    assert_eq!(out.stdout, b"one\n");
    assert_eq!(out.status, Some(4));
    // Audit: one failed and one good password.
    let events = w.accounts.audit.lock().unwrap();
    assert!(events.iter().any(|e| e.kind == sec::audit::Kind::Pw && e.get("ETYP") == "P"));
}

#[test]
fn terminal_shell_echoes_and_edits() {
    let w = world("");
    let mut c = signed_on(&w.server, "qstest");
    let ch = c.session().unwrap();
    assert!(c.pty(ch, "xterm", 80, 24).unwrap());
    assert!(c.shell(ch).unwrap());
    c.send_data(ch, b"echo hx\x7fi\r").unwrap();
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("hi\r\n$") {
        match c.recv().unwrap() {
            Incoming::Data(_, d) => seen.extend(d),
            other => panic!("unexpected {other:?}"),
        }
    }
    let text = String::from_utf8_lossy(&seen);
    assert!(text.contains("echo hx\x08 \x08i\r\n"), "{text:?}");
    c.send_data(ch, b"exit 5\r").unwrap();
    let out = c.collect(ch).unwrap();
    assert_eq!(out.status, Some(5));
}

fn authorize(w: &common::World, user: &str, lines: &str) {
    w.fs.put(&format!("/home/{user}/.ssh/authorized_keys"), lines.as_bytes(), user);
}

#[test]
fn public_key_sign_on() {
    let w = world("");
    let keys: Vec<PrivateKey> = KeyKind::ALL
        .iter()
        .map(|k| PrivateKey::generate(*k).unwrap())
        .collect();
    let text: String = keys.iter().map(|k| k.public().openssh() + " me@pc\n").collect();
    authorize(&w, "QSTEST", &text);
    for k in &keys {
        let mut c = connect(&w.server);
        assert!(c.query_key("qstest", k).unwrap());
        assert!(c.auth_key("qstest", k).unwrap(), "{:?}", k.kind());
        assert_eq!(c.run("echo in", b"").unwrap().stdout, b"in\n");
    }
    // A key not listed, and a listed key for another user.
    let other = PrivateKey::generate(KeyKind::Ed25519).unwrap();
    let mut c = connect(&w.server);
    assert!(!c.query_key("qstest", &other).unwrap());
    assert!(!c.auth_key("qstest", &other).unwrap());
    let mut c = connect(&w.server);
    assert!(!c.auth_key("qstest2", &keys[0]).unwrap());
    // command= forces the command; from= limits the addresses.
    let forced = PrivateKey::generate(KeyKind::Ed25519).unwrap();
    let far = PrivateKey::generate(KeyKind::Ed25519).unwrap();
    authorize(
        &w,
        "QSTEST2",
        &format!(
            "command=\"echo forced $SSH_ORIGINAL_COMMAND\" {}\nfrom=\"192.168.*\" {}\n",
            forced.public().openssh(),
            far.public().openssh()
        ),
    );
    let mut c = connect(&w.server);
    assert!(c.auth_key("qstest2", &forced).unwrap());
    assert_eq!(c.run("echo mine", b"").unwrap().stdout, b"forced echo mine\n");
    let mut c = connect(&w.server);
    assert!(!c.auth_key("qstest2", &far).unwrap());
    let _ = PublicKey::from_blob(&far.public().blob()).unwrap();
}

#[test]
fn max_auth_tries_disconnects() {
    let w = world("MaxAuthTries 3\n");
    let mut c = connect(&w.server);
    let other = PrivateKey::generate(KeyKind::Ed25519).unwrap();
    assert!(!c.auth_key("qstest", &other).unwrap());
    assert!(!c.auth_password("qstest", "bad1").unwrap());
    match c.auth_password("qstest", "bad2") {
        Err(SshError::Disconnected { text, .. }) => {
            assert!(text.contains("Too many authentication failures"));
        }
        other => panic!("expected a disconnect, got {other:?}"),
    }
}

#[test]
fn qmaxsign_disables_the_profile() {
    let w = world("");
    for _ in 0..3 {
        let mut c = connect(&w.server);
        assert!(!c.auth_password("qstest3", "wrong").unwrap());
    }
    assert!(w.accounts.disabled("QSTEST3"));
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qstest3", PASSWORD).unwrap());
    let events = w.accounts.audit.lock().unwrap();
    let pw = events.iter().filter(|e| e.kind == sec::audit::Kind::Pw).count();
    assert!(pw >= 3, "{pw}");
}

#[test]
fn allow_and_deny_users_and_groups() {
    let w = world("AllowUsers qstest qstest2@10.1.*\nDenyUsers qstest2\n");
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qstest2", PASSWORD).unwrap());
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qstest3", PASSWORD).unwrap());
    signed_on(&w.server, "qstest");

    let w = world("AllowUsers *@192.168.*\n");
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qstest", PASSWORD).unwrap());

    let w = world("AllowGroups qstestgrp\n");
    signed_on(&w.server, "qstest");
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qsecofr", PASSWORD).unwrap());

    let w = world("DenyGroups qstestgrp\n");
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qstest", PASSWORD).unwrap());
}

#[test]
fn permit_root_login_governs_qsecofr() {
    let w = world("");
    let mut c = connect(&w.server);
    assert!(!c.auth_password("qsecofr", PASSWORD).unwrap());
    let key = PrivateKey::generate(KeyKind::Ed25519).unwrap();
    authorize(&w, "QSECOFR", &format!("{}\n", key.public().openssh()));
    let mut c = connect(&w.server);
    assert!(c.auth_key("qsecofr", &key).unwrap());

    let w = world("PermitRootLogin yes\n");
    signed_on(&w.server, "qsecofr");

    let w = world("PermitRootLogin no\n");
    authorize(&w, "QSECOFR", &format!("{}\n", key.public().openssh()));
    let mut c = connect(&w.server);
    assert!(!c.auth_key("qsecofr", &key).unwrap());
}

#[test]
fn password_off_and_banner() {
    let w = world("PasswordAuthentication no\nBanner /etc/qsbanner\n");
    w.fs.put("/etc/qsbanner", b"Authorized use only.\n", "QSYS");
    let mut c = connect(&w.server);
    assert_eq!(c.auth_none("qstest").unwrap(), ["publickey"]);
    assert_eq!(c.banner.as_deref(), Some("Authorized use only.\n"));
    assert!(!c.auth_password("qstest", PASSWORD).unwrap());
}

#[test]
fn messages_before_sign_on_are_refused() {
    let w = world("");
    let mut c = connect(&w.server);
    match c.open("session", &[]) {
        Err(SshError::Disconnected { .. } | SshError::Closed) => {}
        other => panic!("expected a disconnect, got {other:?}"),
    }
}

#[test]
fn login_grace_time_ends_idle_connections() {
    let w = world("LoginGraceTime 1\n");
    let mut c = connect(&w.server);
    std::thread::sleep(std::time::Duration::from_millis(2500));
    match c.auth_password("qstest", PASSWORD) {
        Err(SshError::Disconnected { .. } | SshError::Closed | SshError::Io(_)) => {}
        other => panic!("expected a disconnect, got {other:?}"),
    }
}
