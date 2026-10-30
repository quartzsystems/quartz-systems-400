// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The real OpenSSH clients against the daemon on a loopback port:
//! `ssh` exec, `sftp` in batch mode, `scp` over SFTP and the legacy
//! protocol. Ignored by default (it needs `ssh`, `sftp` and `scp` on the
//! machine): `cargo test -p sshd --test openssh -- --ignored`.

#![allow(clippy::many_single_char_names)]

mod common;

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use sshd::Daemon;
use sshd::keys::{KeyKind, PrivateKey};

fn tool(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&std::env::var_os("PATH")?).collect();
    if cfg!(windows) {
        dirs.push(PathBuf::from(r"C:\Windows\System32\OpenSSH"));
    }
    dirs.into_iter().map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// A path as sftp's batch reader takes it (it treats `\` as an escape).
fn slashes(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn common_opts(key: &Path) -> Vec<String> {
    // A throwaway file beside the key: Windows' OpenSSH writes `NUL` as a real file.
    let known = slashes(&key.with_file_name("known_hosts"));
    let mut v: Vec<String> = Vec::new();
    // QS400_SSH_VERBOSE=1 shows the clients' protocol traces on failure.
    if std::env::var_os("QS400_SSH_VERBOSE").is_some() {
        v.push("-vv".into());
    }
    v.extend([
        "-i".into(),
        key.display().to_string(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        format!("UserKnownHostsFile={known}"),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
    ]);
    v
}

#[test]
#[ignore = "needs the OpenSSH clients"]
fn openssh_clients() {
    let (Some(ssh), Some(sftp), Some(scp)) = (tool("ssh"), tool("sftp"), tool("scp")) else {
        eprintln!("OpenSSH clients not found; skipped");
        return;
    };
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let w = common::world(&format!(
        "ListenAddress 127.0.0.1:{port}\n\
         Subsystem sftp /QOpenSys/QIBM/ProdData/SC1/OpenSSH/libexec/sftp-server\n"
    ));
    let key = PrivateKey::generate(KeyKind::EcdsaP256).unwrap();
    w.fs.put(
        "/home/QSTEST/.ssh/authorized_keys",
        format!("{}\n", key.public().openssh()).as_bytes(),
        "QSTEST",
    );
    let dir = std::env::temp_dir().join(format!("qs400-sshd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let key_path = dir.join("id_ecdsa");
    std::fs::write(&key_path, key.to_pem()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let daemon = Daemon::bind(w.server.clone()).unwrap();
    let stop = daemon.stopper();
    std::thread::spawn(move || daemon.run());
    let opts = common_opts(&key_path);
    let target = "qstest@127.0.0.1";

    // ssh exec.
    let out = Command::new(&ssh)
        .args(&opts)
        .args(["-p", &port.to_string(), target, "echo hello $USER"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "hello QSTEST\n",
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // sftp batch: put, get, listing, rename, remove.
    let local = dir.join("up.bin");
    let data: Vec<u8> = (0..300_000u32).map(|i| (i % 253) as u8).collect();
    std::fs::write(&local, &data).unwrap();
    let back = dir.join("back.bin");
    let batch = dir.join("batch.txt");
    std::fs::write(
        &batch,
        format!(
            "put \"{}\" up.bin\nls -l\nget up.bin \"{}\"\nrename up.bin moved.bin\n\
             get /QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR \"{}\"\nrm moved.bin\n",
            slashes(&local),
            slashes(&back),
            slashes(&dir.join("hello.mbr"))
        ),
    )
    .unwrap();
    let out = Command::new(&sftp)
        .args(&opts)
        .args(["-P", &port.to_string(), "-b"])
        .arg(&batch)
        .arg(target)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read(&back).unwrap(), data);
    assert!(std::fs::read(dir.join("hello.mbr")).unwrap().len() >= 160);
    assert!(w.fs.get("/home/QSTEST/moved.bin").is_none());

    // scp over SFTP, then the legacy protocol.
    for legacy in [false, true] {
        let name = if legacy { "legacy.bin" } else { "scp.bin" };
        let mut cmd = Command::new(&scp);
        cmd.args(&opts).args(["-P", &port.to_string()]);
        if legacy {
            cmd.arg("-O");
        }
        let out = cmd.arg(&local).arg(format!("{target}:{name}")).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(w.fs.get(&format!("/home/QSTEST/{name}")).unwrap(), data, "{name}");
    }

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = std::fs::remove_dir_all(&dir);
}
