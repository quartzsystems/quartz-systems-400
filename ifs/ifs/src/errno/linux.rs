// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Linux (`x86_64`) `errno` numbers to IBM's, for the system call
//! boundary: the backend, and later the sockets layer.

use super::Errno;

/// The IBM value for a Linux `errno`; anything unknown is `EUNKNOWN`.
#[must_use]
pub fn from_linux(n: i32) -> Errno {
    use Errno as E;
    match n {
        1 => E::EPERM,
        // 61 is ENODATA: a missing extended attribute.
        2 | 61 => E::ENOENT,
        3 => E::ESRCH,
        4 => E::EINTR,
        5 => E::EIO,
        6 => E::ENXIO,
        7 => E::E2BIG,
        9 => E::EBADF,
        10 => E::ECHILD,
        11 => E::EAGAIN,
        12 => E::ENOMEM,
        13 => E::EACCES,
        14 => E::EFAULT,
        16 => E::EBUSY,
        17 => E::EEXIST,
        18 => E::EXDEV,
        19 => E::ENODEV,
        20 => E::ENOTDIR,
        21 => E::EISDIR,
        22 => E::EINVAL,
        23 => E::ENFILE,
        24 => E::EMFILE,
        25 => E::ENOTTY,
        26 => E::ETXTBSY,
        27 => E::EFBIG,
        28 | 122 => E::ENOSPC,
        29 => E::ESPIPE,
        30 => E::EROFS,
        31 => E::EMLINK,
        32 => E::EPIPE,
        33 => E::EDOM,
        34 => E::ERANGE,
        35 => E::EDEADLK,
        36 => E::ENAMETOOLONG,
        37 => E::ENOLCK,
        38 => E::ENOSYS,
        39 => E::ENOTEMPTY,
        40 => E::ELOOP,
        42 => E::ENOMSG,
        43 => E::EIDRM,
        62 => E::ETIME,
        75 => E::EOVERFLOW,
        84 => E::EILSEQ,
        88 => E::ENOTSOCK,
        89 => E::EDESTADDRREQ,
        90 => E::EMSGSIZE,
        91 => E::EPROTOTYPE,
        92 => E::ENOPROTOOPT,
        93 => E::EPROTONOSUPPORT,
        94 => E::ESOCKTNOSUPPORT,
        95 => E::ENOTSUP,
        96 => E::EPFNOSUPPORT,
        97 => E::EAFNOSUPPORT,
        98 => E::EADDRINUSE,
        99 => E::EADDRNOTAVAIL,
        100 => E::ENETDOWN,
        101 => E::ENETUNREACH,
        102 => E::ENETRESET,
        103 => E::ECONNABORTED,
        104 => E::ECONNRESET,
        105 => E::ENOBUFS,
        106 => E::EISCONN,
        107 => E::ENOTCONN,
        108 => E::ESHUTDOWN,
        109 => E::ETOOMANYREFS,
        110 => E::ETIMEDOUT,
        111 => E::ECONNREFUSED,
        112 => E::EHOSTDOWN,
        113 => E::EHOSTUNREACH,
        114 => E::EALREADY,
        115 => E::EINPROGRESS,
        116 => E::ESTALE,
        117 => E::EDAMAGE,
        125 => E::ECANCELED,
        _ => E::EUNKNOWN,
    }
}

/// The Linux number for an IBM value where Linux has one.
#[must_use]
pub fn to_linux(e: Errno) -> Option<i32> {
    (1..=133).find(|&n| n != 61 && n != 122 && n != 117 && from_linux(n) == e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_values() {
        assert_eq!(from_linux(13), Errno::EACCES);
        assert_eq!(from_linux(40), Errno::ELOOP);
        assert_eq!(from_linux(95), Errno::ENOTSUP);
        assert_eq!(from_linux(9999), Errno::EUNKNOWN);
        assert_eq!(to_linux(Errno::ENOENT), Some(2));
        assert_eq!(to_linux(Errno::ENOSPC), Some(28));
        assert_eq!(to_linux(Errno::ECONVERT), None);
    }
}
