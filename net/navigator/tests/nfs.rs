// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS: the pages against their approved renderings
//! (`tests/golden/nav/nfs-*.html`; `QS400_UPDATE_GOLDEN=1` rewrites them
//! after a deliberate change), and the commands the actions run over the
//! fixture.

#![allow(clippy::many_single_char_names)]

mod common;

use common::{Browser, golden};
use navigator::env::Audit;
use navigator::env::nfs::Nfs;
use qsweb::golden::{Normalize, diff};

/// The page at `route`, served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

/// Compare `served` with approved page `file`, or write it.
fn approved(b: &Browser, served: &str, file: &str) {
    let path = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        let text = served.replace(&b.csrf, "CSRF");
        std::fs::write(&path, text).expect("writes the golden page");
        return;
    }
    let g = golden(file);
    if let Some(d) = diff(served, &g, &Normalize { url: &|_| None }) {
        panic!("{file} differs from the served page at {d}");
    }
}

fn ran(b: &Browser) -> Vec<String> {
    std::mem::take(&mut *b.fx.nfs.ran.lock().unwrap())
}

#[test]
fn pages_are_the_approved_ones() {
    let mut b = Browser::signed_on();
    for (route, file) in [
        ("/nfs", "nav/nfs.html"),
        ("/nfs/add", "nav/nfs-add.html"),
        ("/nfs/exp/1", "nav/nfs-export.html"),
        ("/nfs/exp/1/confirm", "nav/nfs-remove.html"),
        ("/nfs/confirm", "nav/nfs-all.html"),
        ("/nfs/servers", "nav/nfs-servers.html"),
        ("/nfs/servers/ALL/confirm?op=start", "nav/nfs-start.html"),
        ("/nfs/servers/SVR/confirm?op=end", "nav/nfs-end.html"),
        ("/nfs/mounts", "nav/nfs-mounts.html"),
        ("/nfs/mounts/add", "nav/nfs-mount.html"),
        ("/nfs/mounts/101/confirm", "nav/nfs-unmount.html"),
    ] {
        let t = ok(&mut b, route);
        approved(&b, &t, file);
    }
    for key in ["nfs", "nfsservers", "nfsmounts"] {
        assert!(
            navigator::help::TOPICS.iter().any(|t| t.key == key),
            "help {key}"
        );
    }
    assert!(ran(&b).is_empty());
}

fn export_fields<'a>(path: &'a str, ro: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("ok", "OK"),
        ("path", path),
        ("ro", ro),
        ("rw", "admin01 pc0412"),
        ("access", ""),
        ("root", ""),
        ("anon", ""),
        ("nosuid", "*NO"),
        ("vers", "3"),
        ("public", "*NO"),
        ("pseudo", "*NO"),
        ("nfsroot", ""),
        ("exname", ""),
        ("h0", "@10.0.10.0/24"),
        ("d0", "819"),
        ("p0", ""),
        ("w0", "*ASYNC"),
        ("file", "*YES"),
    ]
}

#[test]
fn add_change_and_remove_an_export() {
    let mut b = Browser::signed_on();
    let r = b.post("/nfs/add", &export_fields("/home/projects", "*YES"));
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs"));
    assert_eq!(
        ran(&b),
        [
            "EXPORTFS OPTIONS('-I -F -O ro,rw=admin01:pc0412,vers=3') DIR('/home/projects') HOSTOPT(('@10.0.10.0/24' 819 *ASCII *ASYNC))"
        ]
    );
    let t = ok(&mut b, "/nfs");
    assert!(t.contains("/home/projects") && t.contains("CPCA1C4"), "{t}");
    let e = b.fx.nfs.exports("QSECOFR").unwrap();
    let p = e.iter().find(|e| e.path == "/home/projects").unwrap();
    assert_eq!((p.id, p.versions.as_str()), (4, "3"));
    assert_eq!(p.hosts.len(), 1);
    // Its properties show what it was exported with.
    let t = ok(&mut b, "/nfs/exp/4");
    assert!(t.contains("value=\"admin01:pc0412\""), "{t}");
    // Change: read-write now, without the file.
    let mut f = export_fields("", "*NO");
    f.retain(|(k, _)| *k != "file" && *k != "path");
    f[0] = ("apply", "Apply");
    let r = b.post("/nfs/exp/4", &f);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs/exp/4"));
    assert_eq!(
        ran(&b),
        [
            "EXPORTFS OPTIONS('-I -O rw=admin01:pc0412,vers=3') DIR('/home/projects') HOSTOPT(('@10.0.10.0/24' 819 *ASCII *ASYNC))"
        ]
    );
    // Remove, and from /etc/exports too.
    let r = b.post("/nfs/exp/4/confirm", &[("ok", "OK"), ("file", "*YES")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs"));
    assert_eq!(
        ran(&b),
        ["EXPORTFS OPTIONS('-U -I -F') DIR('/home/projects')"]
    );
    assert_eq!(b.get("/nfs/exp/4").status, 404);
    // Export all puts back what /etc/exports holds.
    let r = b.post("/nfs/confirm", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs"));
    assert_eq!(ran(&b), ["EXPORTFS OPTIONS('-A')"]);
    assert!(ok(&mut b, "/nfs").contains("/www/public"));
}

#[test]
fn a_refused_export_stays_on_its_form() {
    let mut b = Browser::signed_on();
    let r = b.post("/nfs/add", &export_fields("home/x", "*NO"));
    assert_eq!(r.status, 200);
    let t = r.text();
    assert!(
        t.contains("CPFA1C7") && t.contains("value=\"home/x\""),
        "{t}"
    );
}

#[test]
fn without_iosyscfg_nothing_runs() {
    let mut w = Browser::as_user("QPGMR");
    let r = w.post("/nfs/add", &export_fields("/home/x", "*NO"));
    assert_eq!(r.status, 403);
    let r = w.post("/nfs/servers/ALL/confirm?op=end", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(ran(&w).is_empty());
    assert!(
        w.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Refused { object, .. } if object.ends_with("EXPORTFS")))
    );
}

#[test]
fn servers_start_and_end() {
    let mut b = Browser::signed_on();
    let r = b.post("/nfs/servers/SVR/confirm?op=end", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs/servers"));
    assert_eq!(ran(&b), ["ENDNFSSVR SERVER(*SVR)"]);
    assert!(
        !b.fx
            .nfs
            .daemons("QSECOFR")
            .iter()
            .any(|d| d.server == "*SVR")
    );
    let r = b.post("/nfs/servers/ALL/confirm?op=start", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs/servers"));
    assert_eq!(ran(&b), ["STRNFSSVR SERVER(*ALL)"]);
    assert_eq!(b.fx.nfs.daemons("QSECOFR").len(), 7);
    assert!(ok(&mut b, "/nfs/servers").contains("CPCA1BB"));
    assert_eq!(b.get("/nfs/servers/GSS/confirm?op=start").status, 404);
}

#[test]
fn mount_and_unmount() {
    let mut b = Browser::signed_on();
    let r = b.post(
        "/nfs/mounts/add",
        &[
            ("ok", "OK"),
            ("host", "build01"),
            ("path", "/export/src"),
            ("over", "/mnt/src"),
            ("options", "ro,vers=4"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs/mounts"));
    assert_eq!(
        ran(&b),
        ["MOUNT TYPE(*NFS) MFS('build01:/export/src') MNTOVRDIR('/mnt/src') OPTIONS('ro,vers=4')"]
    );
    let t = ok(&mut b, "/nfs/mounts");
    assert!(t.contains("/mnt/src") && t.contains("nfs4"), "{t}");
    let r = b.post("/nfs/mounts/101/confirm", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/nfs/mounts"));
    assert_eq!(ran(&b), ["UNMOUNT TYPE(*NFS) MNTOVRDIR('/mnt/build')"]);
    // The system's own file systems are not unmounted here.
    assert_eq!(b.get("/nfs/mounts/1/confirm").status, 404);
}
