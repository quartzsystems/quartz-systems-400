// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU step 171: `NetServer` against clients nobody here wrote: Samba's
//! `smbclient` and, where the host lets the harness mount, the Linux
//! kernel's CIFS client (`tools/smbtest/tests/interop.rs`). `smbclient`
//! lists the shares over `srvsvc`, round-trips a file on every dialect
//! signed and sealed, prints to the print share and is refused with a
//! wrong password; two kernel mounts see each other's writes and each
//! other's byte-range locks. A host without `smbclient` skips the step
//! and says so (`sudo apt install smbclient cifs-utils`).

use std::path::Path;
use std::process::Command;

use crate::netsvrprobe::{PRINT_SHARE, restart, setup};
use crate::netsvrstep::{PASSWORD, SCRATCH, SHARE, USER, no_client_job};
use crate::qemu::session::Session;
use crate::steps::Suite;
use crate::tcp::{GUEST, forward_port};

/// Whether `sh -c script` succeeds.
fn sh(script: &str) -> bool {
    Command::new("sh")
        .args(["-c", script])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Whether the kernel tests can mount: `mount.cifs`, the file system,
/// and root or a `sudo` that asks nothing.
fn can_mount() -> bool {
    sh("command -v mount.cifs")
        && sh("grep -qw cifs /proc/filesystems || sudo -n modprobe cifs || modprobe cifs")
        && sh("[ \"$(id -u)\" = 0 ] || sudo -n true")
}

/// The count before `word` in cargo's `test result:` line.
fn count(out: &str, word: &str) -> Option<u32> {
    let line = out.lines().rfind(|l| l.starts_with("test result:"))?;
    let at = line.find(word)?;
    line[..at].split_whitespace().last()?.parse().ok()
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 171: Samba's `smbclient` and the kernel CIFS client against
    /// the guest.
    pub(crate) fn netsvr_interop(&self, s: &mut Session) -> Result<String, String> {
        if !sh("command -v smbclient") {
            return Ok("skipped: smbclient is not installed on this host".into());
        }
        setup(s)?;
        restart(s)?;
        no_client_job(s)?;
        let port = forward_port(self, GUEST, 445)?;
        let kernel = can_mount();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut cmd = Command::new(cargo);
        cmd.current_dir(&repo)
            .args([
                "test",
                "--release",
                "--locked",
                "--manifest-path",
                "tools/smbtest/Cargo.toml",
                "--test",
                "interop",
                "--",
                "--ignored",
                "--test-threads",
                "1",
            ])
            .env("CARGO_TARGET_DIR", crate::target_dir().join("smbtest"))
            .env("SMBTEST_HOST", "127.0.0.1")
            .env("SMBTEST_PORT", port.to_string())
            .env("SMBTEST_USER", USER)
            .env("SMBTEST_PASSWORD", PASSWORD)
            .env("SMBTEST_SHARE", SHARE)
            .env("SMBTEST_SCRATCH", SCRATCH)
            .env("SMBTEST_PRINT_SHARE", PRINT_SHARE);
        if !kernel {
            // The mounts need root; Samba's client does not.
            cmd.arg("smbclient_");
        }
        let out = cmd
            .output()
            .map_err(|e| format!("cargo test interop: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let passed = count(&text, " passed").unwrap_or(0);
        if !out.status.success() || passed == 0 || count(&text, " failed") != Some(0) {
            return Err(format!("interop:\n{text}"));
        }
        let version = Command::new("smbclient")
            .arg("--version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        Ok(format!(
            "{passed} interop tests passed with smbclient ({version}){}",
            if kernel {
                " and two kernel CIFS mounts"
            } else {
                "; kernel CIFS mounts skipped (no root)"
            }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::count;

    #[test]
    fn counts_come_from_the_result_line() {
        let out = "running 6 tests\ntest result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.1s\n";
        assert_eq!(count(out, " passed"), Some(6));
        assert_eq!(count(out, " failed"), Some(0));
        assert_eq!(count("no result", " passed"), None);
    }
}
