// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signal numbers: the system's (`QSYSINC/SYS(SIGNAL)`, what programs
//! and the shell use) and Linux's (`x86_64`, what a job's process gets),
//! matched by name.

/// Name and Linux's number; the system's comes from `qsh::signal`.
const TABLE: &[(&str, i32)] = &[
    ("SIGABRT", 6),
    ("SIGFPE", 8),
    ("SIGILL", 4),
    ("SIGINT", 2),
    ("SIGSEGV", 11),
    ("SIGTERM", 15),
    ("SIGUSR1", 10),
    ("SIGUSR2", 12),
    ("SIGALRM", 14),
    ("SIGKILL", 9),
    ("SIGPIPE", 13),
    ("SIGHUP", 1),
    ("SIGQUIT", 3),
    ("SIGSTOP", 19),
    ("SIGTSTP", 20),
    ("SIGCONT", 18),
    ("SIGCHLD", 17),
    ("SIGTTIN", 21),
    ("SIGTTOU", 22),
    ("SIGXCPU", 24),
    ("SIGXFSZ", 25),
    ("SIGVTALRM", 26),
    ("SIGPROF", 27),
    ("SIGURG", 23),
    ("SIGPOLL", 29),
    ("SIGBUS", 7),
    ("SIGWINCH", 28),
    ("SIGTRAP", 5),
    ("SIGSYS", 31),
];

/// Linux's number for the system's.
#[must_use]
pub fn to_linux(sig: i32) -> Option<i32> {
    let name = qsh::signal::name(sig)?;
    let full = if name.starts_with("SIG") {
        name.to_string()
    } else {
        format!("SIG{name}")
    };
    TABLE.iter().find(|(n, _)| *n == full).map(|(_, l)| *l)
}

/// The system's number for Linux's.
#[must_use]
pub fn from_linux(sig: i32) -> i32 {
    TABLE
        .iter()
        .find(|(_, l)| *l == sig)
        .and_then(|(n, _)| qsh::signal::parse(n))
        .unwrap_or(sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_ways() {
        assert_eq!(to_linux(6), Some(15));
        assert_eq!(to_linux(12), Some(9));
        assert_eq!(from_linux(15), 6);
        assert_eq!(from_linux(9), 12);
    }
}
