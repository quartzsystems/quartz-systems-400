// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Qshell in the job: the shell's `Sys` side ([`side`]), `QSH` and
//! `STRQSH` ([`cmd`]), child jobs and `spawn()` ([`child`]), the
//! environment variable commands and the system-level variables
//! ([`env`]), PASE ([`pase`]) and the signal numbers ([`signals`]).
//! The terminal session is `crate::devtools::qsh`.
//!
//! Another job that wants a shell (an SSH session, say) starts a child
//! job running it with [`child::start_shell`].

pub mod child;
pub mod cmd;
pub mod env;
pub mod pase;
pub mod side;
pub mod signals;

use qsh_job::Setup;

use crate::console::host::Host;

/// `LIB` and `NAME` of `/QSYS.LIB/LIB.LIB/NAME.TYPE` (or a `LIB/NAME`
/// name), upper case, when the type is `ext`.
pub fn object_of(path: &str, ext: &str) -> Option<(String, String)> {
    let up = path.to_ascii_uppercase();
    if let Some(rest) = up.strip_prefix("/QSYS.LIB/") {
        let parts: Vec<&str> = rest.split('/').collect();
        let (lib, obj) = match parts.as_slice() {
            [o] => ("QSYS", *o),
            [l, o] => (l.strip_suffix(".LIB")?, *o),
            _ => return None,
        };
        return Some((lib.to_string(), obj.strip_suffix(ext)?.to_string()));
    }
    let (lib, obj) = up.split_once('/').unwrap_or(("*LIBL", up.as_str()));
    Some((lib.to_string(), obj.to_string()))
}

/// The variables every shell of the job starts with, before the job's
/// own: Qshell's set (`tools/conformance/oracle/net/pub400/qsh-env.txt`)
/// with this machine's values.
fn base_env(host: &Host, home: &str, terminal: &str) -> Vec<(String, String)> {
    let name = crate::display::dds::system_name(host);
    let v = |k: &str, x: &str| (k.to_string(), x.to_string());
    vec![
        v("HOME", home),
        v("HOSTNAME", &name),
        v("HOSTTYPE", "x86_64"),
        v("LOGNAME", &host.start.user),
        v("MACHTYPE", "x86_64-qs400-os400"),
        v("OSTYPE", "os400"),
        v("PATH", "/usr/bin:.:/QOpenSys/usr/bin"),
        v("QIBM_DESCRIPTOR_STDERR", "CRLN=N"),
        v("QIBM_DESCRIPTOR_STDIN", "CRLN=Y"),
        v("QIBM_DESCRIPTOR_STDOUT", "CRLN=N"),
        v("QIBM_USE_DESCRIPTOR_STDIO", "Y"),
        v("SHLVL", "1"),
        v("TERMINAL_TYPE", terminal),
        v("TRACEOPT", "UNLINK"),
    ]
}

/// How a shell of this job starts: the job's environment variables over
/// Qshell's base set, the user's home as the directory when it exists.
/// `terminal` is `TERMINAL_TYPE` (`5250`, `PIPELINE`).
pub fn setup(host: &Host, terminal: &str) -> Setup {
    let ids = side::profiles(host);
    let home = ids
        .iter()
        .find(|i| i.name == host.start.user)
        .map_or_else(|| format!("/home/{}", host.start.user), |i| i.home.clone());
    let mut env = base_env(host, &home, terminal);
    for (k, val) in sysapi::envproc::job_pairs() {
        if let Some(e) = env.iter_mut().find(|(n, _)| *n == k) {
            e.1 = val;
        } else {
            env.push((k, val));
        }
    }
    let cwd = {
        use ifs::client::IfsClient;
        let c = stg::remote::RemoteIfs::new(host.link.clone());
        match c.stat(&home, true) {
            Ok(s) if s.mode & ifs::ops::flags::S_IFMT == ifs::ops::flags::S_IFDIR => home,
            _ => "/".to_string(),
        }
    };
    Setup {
        env,
        cwd,
        pid: i32::try_from(std::process::id()).unwrap_or(0),
        nodename: crate::display::dds::system_name(host),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_paths() {
        assert_eq!(
            object_of("/qsys.lib/qgpl.lib/cfg.dtaara", ".DTAARA"),
            Some(("QGPL".into(), "CFG".into()))
        );
        assert_eq!(object_of("qgpl/x", ".DTAARA"), Some(("QGPL".into(), "X".into())));
        assert_eq!(object_of("/QSYS.LIB/QGPL.LIB/X.PGM", ".DTAARA"), None);
    }
}
