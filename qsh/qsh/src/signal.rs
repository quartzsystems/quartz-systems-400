// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signal names and numbers. The numbers are the system's
//! (`QSYSINC/SYS(SIGNAL)`), not Linux's: scripts on the system write
//! `kill -6` and `trap ... 4` with these values.

/// `(number, name without SIG)`, in `kill -l` order.
pub const SIGNALS: &[(i32, &str)] = &[
    (1, "ABRT"),
    (2, "FPE"),
    (3, "ILL"),
    (4, "INT"),
    (5, "SEGV"),
    (6, "TERM"),
    (7, "USR1"),
    (8, "USR2"),
    (9, "IO"),
    (10, "ALL"),
    (11, "OTHER"),
    (12, "KILL"),
    (13, "PIPE"),
    (14, "ALRM"),
    (15, "HUP"),
    (16, "QUIT"),
    (17, "STOP"),
    (18, "TSTP"),
    (19, "CONT"),
    (20, "CHLD"),
    (21, "TTIN"),
    (22, "TTOU"),
    (23, "URG"),
    (24, "POLL"),
    (25, "PCANCEL"),
    (26, "PALRM"),
    (27, "WINCH"),
    (32, "BUS"),
    (33, "DANGER"),
    (34, "PRE"),
    (35, "SYS"),
    (36, "TRAP"),
    (37, "PROF"),
    (38, "VTALRM"),
    (39, "XCPU"),
    (40, "XFSZ"),
];

/// `SIGINT`.
pub const SIGINT: i32 = 4;
/// `SIGTERM`.
pub const SIGTERM: i32 = 6;
/// `SIGKILL`.
pub const SIGKILL: i32 = 12;
/// `SIGPIPE`.
pub const SIGPIPE: i32 = 13;
/// `SIGHUP`.
pub const SIGHUP: i32 = 15;
/// `SIGQUIT`.
pub const SIGQUIT: i32 = 16;
/// `SIGSTOP`.
pub const SIGSTOP: i32 = 17;
/// `SIGTSTP`.
pub const SIGTSTP: i32 = 18;
/// `SIGCONT`.
pub const SIGCONT: i32 = 19;
/// `SIGCHLD`.
pub const SIGCHLD: i32 = 20;

/// The highest signal number.
pub const SIGMAX: i32 = 63;

/// The name of a signal number, without `SIG`.
pub fn name(n: i32) -> Option<&'static str> {
    SIGNALS.iter().find(|(k, _)| *k == n).map(|(_, s)| *s)
}

/// A signal from a name (with or without `SIG`, any case) or number.
/// `EXIT` and `0` are signal 0, the trap at shell exit.
pub fn parse(s: &str) -> Option<i32> {
    if let Ok(n) = s.parse::<i32>() {
        return (n == 0 || name(n).is_some()).then_some(n);
    }
    let up = s.to_ascii_uppercase();
    let bare = up.strip_prefix("SIG").unwrap_or(&up);
    if bare == "EXIT" {
        return Some(0);
    }
    SIGNALS.iter().find(|(_, n)| *n == bare).map(|(k, _)| *k)
}

/// The exit status of a command ended by signal `n`.
pub fn status(n: i32) -> i32 {
    128 + n
}
