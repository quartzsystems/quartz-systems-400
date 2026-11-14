// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server's files in the IFS and its library
//! (docs/DIRSRV.md §4, §10): `/QIBM/UserData/OS400/DirSrv/` with the
//! instance list and `QUSRDIR`'s `ibmslapd.conf`, made when missing as
//! the IFS starts (a first IPL, or the first of V1R2M0 after a slip
//! install); `QUSRDIRDB`, the instances' entry stores' library, made at
//! each IPL when missing. A changed file is the customer's.

use dirsrv::config::{self, InstanceList, shipped};
use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::RenameMode;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_TRUNC, O_WRONLY};
use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::System;

/// The library of the instances' entry stores (`*DIRDB`).
pub const LIBRARY: &str = "QUSRDIRDB";

/// The profile the instance jobs run as, owner of their files.
pub const OWNER: &str = "QDIRSRV";

/// `QUSRDIRDB`, unless it exists; whether it was made.
///
/// # Errors
///
/// The store's.
pub fn ensure_library(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    stamp: &wm::obj::Stamp,
) -> Result<bool, String> {
    let name = Name::parse(LIBRARY).expect("valid");
    if ops::find_library(txn, qsys, name).is_ok() {
        return Ok(false);
    }
    ops::create(
        txn,
        &CreateParams {
            library: qsys,
            name,
            type_code: types::LIB,
            attribute: "PROD".into(),
            // Blank, as the primary's.
            text: String::new(),
            owner: "QSYS".into(),
            system: stamp.system.clone(),
            now: stamp.now,
            by_system: true,
        },
    )
    .map_err(|e| format!("{LIBRARY}: {e}"))?;
    Ok(true)
}

/// The system's DNS name, or its system name without one: the
/// projected suffix's `os400-sys=` value.
fn dns_name(system: &mut System) -> String {
    let fqdn = super::tcp::config(system)
        .map(|c| c.domain.fqdn())
        .unwrap_or_default();
    if fqdn.is_empty() {
        system.system_name()
    } else {
        fqdn
    }
}

/// Make the shipped files that are missing.
pub fn ship(system: &mut System) {
    let dns = dns_name(system);
    super::certs::as_system(system, |c| {
        let conf = config::conf_path(config::ROOT, config::DEFAULT_INSTANCE);
        let files: [(&str, u32, u32, String); 2] = [
            (
                config::INSTANCES,
                config::INSTANCES_CCSID,
                0o644,
                shipped::INSTANCES.to_string(),
            ),
            (
                conf.as_str(),
                config::CONF_CCSID,
                0o600,
                shipped::slapd_conf(config::DEFAULT_INSTANCE, &dns),
            ),
        ];
        for (path, ccsid, mode, text) in files {
            if let Err(e) = create(c, path, ccsid, mode, text.as_bytes()) {
                eprintln!("qsstg: {e}");
            }
        }
    });
}

/// Make `dir` and its parents, readable by everyone.
fn make_dirs(c: &dyn IfsClient, dir: &str) -> Result<(), String> {
    let mut at = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        at.push('/');
        at.push_str(part);
        match c.mkdir(&at, 0o755) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(e) => return Err(format!("{at}: {}", e.text())),
        }
    }
    Ok(())
}

/// The directory of `path`.
fn parent(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

/// Make `path` with `data` unless it exists; an owner-only file is
/// given to [`OWNER`].
fn create(c: &dyn IfsClient, path: &str, ccsid: u32, mode: u32, data: &[u8]) -> Result<(), String> {
    make_dirs(c, parent(path))?;
    let fd = match c.open(
        path,
        OpenArgs::new(O_WRONLY | O_CREAT | O_EXCL | O_CCSID)
            .mode(mode)
            .conv(ccsid),
    ) {
        Ok(fd) => fd,
        Err(Errno::EEXIST) => return Ok(()),
        Err(e) => return Err(format!("{path}: {}", e.text())),
    };
    let w = ifs::client::write_all(c, fd, data);
    let _ = c.close(fd);
    w.map_err(|e| format!("{path}: {}", e.text()))?;
    if mode.trailing_zeros() >= 6 {
        c.chgown(path, OWNER, false)
            .map_err(|e| format!("{path}: {}", e.text()))?;
    }
    Ok(())
}

/// A file's text; `None` when it does not exist.
///
/// # Errors
///
/// The IFS's, as text.
pub fn read(system: &mut System, path: &str) -> Result<Option<String>, String> {
    super::certs::as_system(system, |c| super::certs::read_file(c, path))
        .map(|b| b.map(|b| String::from_utf8_lossy(&b).into_owned()))
}

/// Replace a configuration file: written beside it, then renamed over
/// it, owner-only and [`OWNER`]'s.
///
/// # Errors
///
/// The IFS's, as text.
pub fn write(system: &mut System, path: &str, text: &str) -> Result<(), String> {
    replace(system, path, text, 0o600, config::CONF_CCSID)
}

/// Replace the instance list, readable by everyone and [`OWNER`]'s.
///
/// # Errors
///
/// The IFS's, as text.
pub fn write_instances(system: &mut System, text: &str) -> Result<(), String> {
    replace(
        system,
        config::INSTANCES,
        text,
        0o644,
        config::INSTANCES_CCSID,
    )
}

fn replace(system: &mut System, path: &str, text: &str, mode: u32, ccsid: u32) -> Result<(), String> {
    super::certs::as_system(system, |c| {
        let tmp = format!("{path}.new");
        let fd = c
            .open(
                &tmp,
                OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
                    .mode(mode)
                    .conv(ccsid),
            )
            .map_err(|e| format!("{tmp}: {}", e.text()))?;
        let w = ifs::client::write_all(c, fd, text.as_bytes()).and_then(|()| c.fsync(fd, false));
        let _ = c.close(fd);
        w.map_err(|e| format!("{tmp}: {}", e.text()))?;
        c.rename(&tmp, path, RenameMode::Unlink)
            .map_err(|e| format!("{path}: {}", e.text()))?;
        c.chgown(path, OWNER, false)
            .map_err(|e| format!("{path}: {}", e.text()))
    })
}

/// The instance list; the shipped one when the file is missing.
///
/// # Errors
///
/// `GLD0127` when it cannot be read or is not LDIF.
pub fn instances(system: &mut System) -> Result<InstanceList, config::ConfigError> {
    let file = |why: String| config::ConfigError::File {
        file: config::INSTANCES.into(),
        why,
    };
    match read(system, config::INSTANCES).map_err(file)? {
        Some(t) => InstanceList::parse(&t),
        None => InstanceList::parse(shipped::INSTANCES),
    }
}
