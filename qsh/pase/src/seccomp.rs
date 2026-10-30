// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The seccomp filter a PASE process runs under, written as a classic
//! BPF program: anything but `x86_64` is killed, the system calls a
//! userland needs are allowed, every other one fails with `EPERM`
//! (mounting, tracing, modules, rebooting, keyrings, BPF, namespaces,
//! `io_uring`, pidfds, the clock ...). [`check`] runs a program the way
//! the kernel does, so the filter is tested on any host.

/// One BPF instruction (`struct sock_filter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Insn {
    /// The operation.
    pub code: u16,
    /// Jump offset when true.
    pub jt: u8,
    /// Jump offset when false.
    pub jf: u8,
    /// The operand.
    pub k: u32,
}

/// `BPF_LD | BPF_W | BPF_ABS`.
pub const LD_ABS: u16 = 0x20;
/// `BPF_JMP | BPF_JEQ | BPF_K`.
pub const JEQ: u16 = 0x15;
/// `BPF_JMP | BPF_JGE | BPF_K`.
pub const JGE: u16 = 0x35;
/// `BPF_RET | BPF_K`.
pub const RET: u16 = 0x06;

/// `AUDIT_ARCH_X86_64`.
pub const ARCH_X86_64: u32 = 0xC000_003E;
/// `SECCOMP_RET_KILL_PROCESS`.
pub const RET_KILL: u32 = 0x8000_0000;
/// `SECCOMP_RET_ERRNO`.
pub const RET_ERRNO: u32 = 0x0005_0000;
/// `SECCOMP_RET_ALLOW`.
pub const RET_ALLOW: u32 = 0x7fff_0000;
/// Linux `EPERM`.
pub const EPERM: u32 = 1;
/// The x32 ABI's bit in a system call number.
pub const X32_BIT: u32 = 0x4000_0000;

/// Offset of `nr` in `struct seccomp_data`.
const NR: u32 = 0;
/// Offset of `arch`.
const ARCH: u32 = 4;

/// The system calls a PASE program may make: name and `x86_64` number.
pub const ALLOWED: &[(&str, u32)] = &[
    ("read", 0),
    ("write", 1),
    ("open", 2),
    ("close", 3),
    ("stat", 4),
    ("fstat", 5),
    ("lstat", 6),
    ("poll", 7),
    ("lseek", 8),
    ("mmap", 9),
    ("mprotect", 10),
    ("munmap", 11),
    ("brk", 12),
    ("rt_sigaction", 13),
    ("rt_sigprocmask", 14),
    ("rt_sigreturn", 15),
    ("ioctl", 16),
    ("pread64", 17),
    ("pwrite64", 18),
    ("readv", 19),
    ("writev", 20),
    ("access", 21),
    ("pipe", 22),
    ("select", 23),
    ("sched_yield", 24),
    ("mremap", 25),
    ("msync", 26),
    ("mincore", 27),
    ("madvise", 28),
    ("shmget", 29),
    ("shmat", 30),
    ("shmctl", 31),
    ("dup", 32),
    ("dup2", 33),
    ("pause", 34),
    ("nanosleep", 35),
    ("getitimer", 36),
    ("alarm", 37),
    ("setitimer", 38),
    ("getpid", 39),
    ("sendfile", 40),
    ("socket", 41),
    ("connect", 42),
    ("accept", 43),
    ("sendto", 44),
    ("recvfrom", 45),
    ("sendmsg", 46),
    ("recvmsg", 47),
    ("shutdown", 48),
    ("bind", 49),
    ("listen", 50),
    ("getsockname", 51),
    ("getpeername", 52),
    ("socketpair", 53),
    ("setsockopt", 54),
    ("getsockopt", 55),
    ("clone", 56),
    ("fork", 57),
    ("vfork", 58),
    ("execve", 59),
    ("exit", 60),
    ("wait4", 61),
    ("kill", 62),
    ("uname", 63),
    ("semget", 64),
    ("semop", 65),
    ("semctl", 66),
    ("shmdt", 67),
    ("msgget", 68),
    ("msgsnd", 69),
    ("msgrcv", 70),
    ("msgctl", 71),
    ("fcntl", 72),
    ("flock", 73),
    ("fsync", 74),
    ("fdatasync", 75),
    ("truncate", 76),
    ("ftruncate", 77),
    ("getdents", 78),
    ("getcwd", 79),
    ("chdir", 80),
    ("fchdir", 81),
    ("rename", 82),
    ("mkdir", 83),
    ("rmdir", 84),
    ("creat", 85),
    ("link", 86),
    ("unlink", 87),
    ("symlink", 88),
    ("readlink", 89),
    ("chmod", 90),
    ("fchmod", 91),
    ("chown", 92),
    ("fchown", 93),
    ("lchown", 94),
    ("umask", 95),
    ("gettimeofday", 96),
    ("getrlimit", 97),
    ("getrusage", 98),
    ("sysinfo", 99),
    ("times", 100),
    ("getuid", 102),
    ("getgid", 104),
    ("setuid", 105),
    ("setgid", 106),
    ("geteuid", 107),
    ("getegid", 108),
    ("setpgid", 109),
    ("getppid", 110),
    ("getpgrp", 111),
    ("setsid", 112),
    ("setreuid", 113),
    ("setregid", 114),
    ("getgroups", 115),
    ("setgroups", 116),
    ("setresuid", 117),
    ("getresuid", 118),
    ("setresgid", 119),
    ("getresgid", 120),
    ("getpgid", 121),
    ("setfsuid", 122),
    ("setfsgid", 123),
    ("getsid", 124),
    ("capget", 125),
    ("rt_sigpending", 127),
    ("rt_sigtimedwait", 128),
    ("rt_sigqueueinfo", 129),
    ("rt_sigsuspend", 130),
    ("sigaltstack", 131),
    ("utime", 132),
    ("mknod", 133),
    ("statfs", 137),
    ("fstatfs", 138),
    ("getpriority", 140),
    ("setpriority", 141),
    ("sched_setparam", 142),
    ("sched_getparam", 143),
    ("sched_setscheduler", 144),
    ("sched_getscheduler", 145),
    ("sched_get_priority_max", 146),
    ("sched_get_priority_min", 147),
    ("sched_rr_get_interval", 148),
    ("mlock", 149),
    ("munlock", 150),
    ("prctl", 157),
    ("arch_prctl", 158),
    ("setrlimit", 160),
    ("sync", 162),
    ("gettid", 186),
    ("readahead", 187),
    ("getxattr", 191),
    ("lgetxattr", 192),
    ("fgetxattr", 193),
    ("listxattr", 194),
    ("llistxattr", 195),
    ("flistxattr", 196),
    ("tkill", 200),
    ("time", 201),
    ("futex", 202),
    ("sched_setaffinity", 203),
    ("sched_getaffinity", 204),
    ("epoll_create", 213),
    ("getdents64", 217),
    ("set_tid_address", 218),
    ("restart_syscall", 219),
    ("semtimedop", 220),
    ("fadvise64", 221),
    ("timer_create", 222),
    ("timer_settime", 223),
    ("timer_gettime", 224),
    ("timer_getoverrun", 225),
    ("timer_delete", 226),
    ("clock_gettime", 228),
    ("clock_getres", 229),
    ("clock_nanosleep", 230),
    ("exit_group", 231),
    ("epoll_wait", 232),
    ("epoll_ctl", 233),
    ("tgkill", 234),
    ("utimes", 235),
    ("waitid", 247),
    ("inotify_init", 253),
    ("inotify_add_watch", 254),
    ("inotify_rm_watch", 255),
    ("openat", 257),
    ("mkdirat", 258),
    ("mknodat", 259),
    ("fchownat", 260),
    ("futimesat", 261),
    ("newfstatat", 262),
    ("unlinkat", 263),
    ("renameat", 264),
    ("linkat", 265),
    ("symlinkat", 266),
    ("readlinkat", 267),
    ("fchmodat", 268),
    ("faccessat", 269),
    ("pselect6", 270),
    ("ppoll", 271),
    ("set_robust_list", 273),
    ("get_robust_list", 274),
    ("splice", 275),
    ("tee", 276),
    ("sync_file_range", 277),
    ("vmsplice", 278),
    ("utimensat", 280),
    ("epoll_pwait", 281),
    ("signalfd", 282),
    ("timerfd_create", 283),
    ("eventfd", 284),
    ("fallocate", 285),
    ("timerfd_settime", 286),
    ("timerfd_gettime", 287),
    ("accept4", 288),
    ("signalfd4", 289),
    ("eventfd2", 290),
    ("epoll_create1", 291),
    ("dup3", 292),
    ("pipe2", 293),
    ("inotify_init1", 294),
    ("preadv", 295),
    ("pwritev", 296),
    ("rt_tgsigqueueinfo", 297),
    ("recvmmsg", 299),
    ("prlimit64", 302),
    ("syncfs", 306),
    ("sendmmsg", 307),
    ("getcpu", 309),
    ("sched_setattr", 314),
    ("sched_getattr", 315),
    ("renameat2", 316),
    ("getrandom", 318),
    ("memfd_create", 319),
    ("execveat", 322),
    ("membarrier", 324),
    ("mlock2", 325),
    ("copy_file_range", 326),
    ("preadv2", 327),
    ("pwritev2", 328),
    ("statx", 332),
    ("rseq", 334),
    ("clone3", 435),
    ("close_range", 436),
    ("openat2", 437),
    ("faccessat2", 439),
    ("epoll_pwait2", 441),
    ("futex_waitv", 449),
];

/// The filter.
#[must_use]
pub fn program() -> Vec<Insn> {
    let n = ALLOWED.len();
    assert!(n < 255, "the allow list must fit one jump");
    let mut p = vec![
        Insn { code: LD_ABS, jt: 0, jf: 0, k: ARCH },
        Insn { code: JEQ, jt: 1, jf: 0, k: ARCH_X86_64 },
        Insn { code: RET, jt: 0, jf: 0, k: RET_KILL },
        Insn { code: LD_ABS, jt: 0, jf: 0, k: NR },
        // The x32 ABI's numbers are refused.
        Insn {
            code: JGE,
            jt: u8::try_from(n).expect("checked"),
            jf: 0,
            k: X32_BIT,
        },
    ];
    for (i, (_, nr)) in ALLOWED.iter().enumerate() {
        p.push(Insn {
            code: JEQ,
            jt: u8::try_from(n - i).expect("checked"),
            jf: 0,
            k: *nr,
        });
    }
    p.push(Insn { code: RET, jt: 0, jf: 0, k: RET_ERRNO | EPERM });
    p.push(Insn { code: RET, jt: 0, jf: 0, k: RET_ALLOW });
    p
}

/// Run a filter over one call, as the kernel would: the action.
#[must_use]
pub fn check(p: &[Insn], arch: u32, nr: u32) -> u32 {
    let mut a = 0u32;
    let mut pc = 0usize;
    while let Some(i) = p.get(pc) {
        match i.code {
            LD_ABS => {
                a = match i.k {
                    NR => nr,
                    ARCH => arch,
                    _ => 0,
                };
                pc += 1;
            }
            JEQ | JGE => {
                let hit = if i.code == JEQ { a == i.k } else { a >= i.k };
                pc += 1 + usize::from(if hit { i.jt } else { i.jf });
            }
            RET => return i.k,
            _ => return RET_KILL,
        }
    }
    RET_KILL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nr(name: &str) -> u32 {
        ALLOWED.iter().find(|(n, _)| *n == name).unwrap().1
    }

    #[test]
    fn allowed_calls_pass_and_others_fail() {
        let p = program();
        for (_, n) in ALLOWED {
            assert_eq!(check(&p, ARCH_X86_64, *n), RET_ALLOW, "{n}");
        }
        // ptrace, mount, reboot, init_module, bpf, unshare, setns,
        // io_uring_setup, pidfd_open, settimeofday, kexec_load.
        for n in [101, 165, 169, 175, 321, 272, 308, 425, 434, 164, 246] {
            assert_eq!(check(&p, ARCH_X86_64, n), RET_ERRNO | EPERM, "{n}");
        }
        assert_eq!(check(&p, ARCH_X86_64, X32_BIT | nr("read")), RET_ERRNO | EPERM);
        assert_eq!(check(&p, 0x4000_0003, nr("read")), RET_KILL);
    }

    #[test]
    fn the_list_is_sorted_and_unique() {
        for w in ALLOWED.windows(2) {
            assert!(w[0].1 < w[1].1, "{} {}", w[0].0, w[1].0);
        }
        assert!(program().len() < 300);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn numbers_are_the_kernels() {
        for (name, n) in [
            ("read", libc::SYS_read),
            ("openat", libc::SYS_openat),
            ("execve", libc::SYS_execve),
            ("exit_group", libc::SYS_exit_group),
            ("statx", libc::SYS_statx),
            ("clone3", libc::SYS_clone3),
            ("getrandom", libc::SYS_getrandom),
            ("rseq", libc::SYS_rseq),
            ("futex_waitv", libc::SYS_futex_waitv),
        ] {
            assert_eq!(i64::from(nr(name)), n, "{name}");
        }
    }
}
