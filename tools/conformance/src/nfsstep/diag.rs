// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a guest shows when an NFS step failed, written beside the
//! suite's other output so the failure is read without a rerun.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use crate::qemu::session::Session;

/// The commands whose answers are kept, in order.
fn commands() -> Vec<String> {
    let mut v: Vec<String> = [
        "svc.dspjoblog",
        "DSPMSG MSGQ(QSYSOPR)",
        "wrkactjob",
        "svc.sql SELECT * FROM QSYS2/NFS_EXPORT_INFO;",
        "svc.sql SELECT * FROM QSYS2/MOUNTED_FS_INFO;",
        "svc.sql SELECT * FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT IN (111, 2049);",
        "svc.ifs stat /etc/RPCBTAB",
        "svc.ifs ls /QIBM/UserData/OS400/NFS",
        "svc.tcp ifc",
        "svc.tcp host",
    ]
    .iter()
    .map(|c| (*c).to_string())
    .collect();
    v.extend(
        super::DAEMONS
            .iter()
            .map(|d| format!("svc.dspjoblog JOB({d})")),
    );
    v
}

/// Writes `nfs-<step>-<guest>.txt` with the answers and `extra` (what the
/// host saw); its path. A guest that stops answering ends the list.
pub(crate) fn dump(dir: &Path, step: &str, guest: &str, s: &mut Session, extra: &str) -> String {
    let mut text = String::new();
    if !extra.is_empty() {
        let _ = write!(text, "=== the host\n{extra}\n");
    }
    for c in commands() {
        let _ = writeln!(text, "=== {c}");
        match s.command(&c, Duration::from_secs(30)) {
            Ok(out) => {
                let _ = writeln!(text, "{out}");
            }
            Err(e) => {
                let _ = writeln!(text, "(no answer: {e})");
                break;
            }
        }
    }
    let lines: Vec<&str> = s.log.lines().collect();
    let _ = write!(
        text,
        "=== the serial log, last 200 lines\n{}\n",
        lines[lines.len().saturating_sub(200)..].join("\n")
    );
    // The whole serial log beside it: the daemons' and the service's
    // notes come long before the commands above.
    let _ = std::fs::write(dir.join(format!("nfs-{step}-{guest}.serial.txt")), &s.log);
    let path = dir.join(format!("nfs-{step}-{guest}.txt"));
    match std::fs::write(&path, text) {
        Ok(()) => path.display().to_string(),
        Err(e) => format!("{} not written: {e}", path.display()),
    }
}
