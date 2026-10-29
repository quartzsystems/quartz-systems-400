// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The C-style procedures the runtime provides under IBM's service
//! program names: sockets (`QSOSRV1`), the resolver (`QSOSRV2`), the
//! descriptor calls shared with the IFS (`QP0LLIB1`) and `errno`
//! (`QC2UTIL1`). An import of one is a `*SYSTEM` import like `QLEAWI`'s;
//! `DSPPGM` names the service program IBM binds it from. Names are
//! matched without regard to case: the RPG compiler upper-cases every
//! `EXTPROC` name, C-style headers spell them in lower case.

/// Export name (as IBM spells it) and the service program.
pub const SYSTEM_SRVPGM_PROCS: &[(&str, &str)] = &[
    // QSOSRV1: the socket calls, BSD 4.3 names and the 4.4 (`_XOPEN_SOURCE`) forms.
    ("socket", "QSOSRV1"),
    ("socketpair", "QSOSRV1"),
    ("bind", "QSOSRV1"),
    ("Rbind", "QSOSRV1"),
    ("listen", "QSOSRV1"),
    ("accept", "QSOSRV1"),
    ("accept_and_recv", "QSOSRV1"),
    ("connect", "QSOSRV1"),
    ("send", "QSOSRV1"),
    ("sendto", "QSOSRV1"),
    ("sendmsg", "QSOSRV1"),
    ("recv", "QSOSRV1"),
    ("recvfrom", "QSOSRV1"),
    ("recvmsg", "QSOSRV1"),
    ("shutdown", "QSOSRV1"),
    ("getsockopt", "QSOSRV1"),
    ("setsockopt", "QSOSRV1"),
    ("getsockname", "QSOSRV1"),
    ("getpeername", "QSOSRV1"),
    ("poll", "QSOSRV1"),
    ("givedescriptor", "QSOSRV1"),
    ("takedescriptor", "QSOSRV1"),
    ("gethostname", "QSOSRV1"),
    ("qso_socket98", "QSOSRV1"),
    ("qso_socketpair98", "QSOSRV1"),
    ("qso_bind98", "QSOSRV1"),
    ("qso_Rbind98", "QSOSRV1"),
    ("qso_listen98", "QSOSRV1"),
    ("qso_accept98", "QSOSRV1"),
    ("qso_accept_and_recv98", "QSOSRV1"),
    ("qso_connect98", "QSOSRV1"),
    ("qso_send98", "QSOSRV1"),
    ("qso_sendto98", "QSOSRV1"),
    ("qso_sendmsg98", "QSOSRV1"),
    ("qso_recv98", "QSOSRV1"),
    ("qso_recvfrom98", "QSOSRV1"),
    ("qso_recvmsg98", "QSOSRV1"),
    ("qso_shutdown98", "QSOSRV1"),
    ("qso_getsockopt98", "QSOSRV1"),
    ("qso_setsockopt98", "QSOSRV1"),
    ("qso_getsockname98", "QSOSRV1"),
    ("qso_getpeername98", "QSOSRV1"),
    // Macros in IBM's <netinet/in.h>; exported here so prototypes bind.
    ("htonl", "QSOSRV1"),
    ("htons", "QSOSRV1"),
    ("ntohl", "QSOSRV1"),
    ("ntohs", "QSOSRV1"),
    // QSOSRV2: addresses and the resolver.
    ("inet_addr", "QSOSRV2"),
    ("inet_ntoa", "QSOSRV2"),
    ("inet_ntoa_r", "QSOSRV2"),
    ("inet_pton", "QSOSRV2"),
    ("inet_ntop", "QSOSRV2"),
    ("gethostbyname", "QSOSRV2"),
    ("gethostbyname_r", "QSOSRV2"),
    ("gethostbyaddr", "QSOSRV2"),
    ("gethostbyaddr_r", "QSOSRV2"),
    ("getaddrinfo", "QSOSRV2"),
    ("freeaddrinfo", "QSOSRV2"),
    ("getnameinfo", "QSOSRV2"),
    ("gai_strerror", "QSOSRV2"),
    ("res_init", "QSOSRV2"),
    ("res_query", "QSOSRV2"),
    ("res_search", "QSOSRV2"),
    ("res_close", "QSOSRV2"),
    ("getservbyname", "QSOSRV2"),
    ("getservbyport", "QSOSRV2"),
    ("hstrerror", "QSOSRV2"),
    ("__h_errno", "QSOSRV2"),
    ("qso_inet_addr98", "QSOSRV2"),
    ("qso_gethostbyname98", "QSOSRV2"),
    ("qso_gethostbyname_r98", "QSOSRV2"),
    ("qso_gethostbyaddr98", "QSOSRV2"),
    ("qso_gethostbyaddr_r98", "QSOSRV2"),
    ("qso_getaddrinfo98", "QSOSRV2"),
    ("qso_getnameinfo98", "QSOSRV2"),
    ("qso_getservbyname98", "QSOSRV2"),
    ("qso_getservbyport98", "QSOSRV2"),
    // QP0LLIB1: descriptor calls sockets and IFS files share.
    ("read", "QP0LLIB1"),
    ("write", "QP0LLIB1"),
    ("readv", "QP0LLIB1"),
    ("writev", "QP0LLIB1"),
    ("close", "QP0LLIB1"),
    ("select", "QP0LLIB1"),
    ("fcntl", "QP0LLIB1"),
    ("ioctl", "QP0LLIB1"),
    ("dup", "QP0LLIB1"),
    ("dup2", "QP0LLIB1"),
    // QC2UTIL1: errno.
    ("__errno", "QC2UTIL1"),
    ("strerror", "QC2UTIL1"),
];

/// The service program a `*SYSTEM` import is shown under: `QLEAWI` for
/// the runtime's own procedures, the RPG runtime's for `_QRNX_`, the
/// table above for the C-style ones; `None` for a name no table has.
#[must_use]
pub fn system_srvpgm(name: &str) -> Option<&'static str> {
    if super::SYSTEM_PROCS.contains(&name) {
        return Some("QLEAWI");
    }
    if name.starts_with(super::RPG_RUNTIME_PREFIX) {
        return Some("QRNXIE");
    }
    SYSTEM_SRVPGM_PROCS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, s)| *s)
}

/// Whether a name is a C-style system procedure of the table.
#[must_use]
pub fn is_system_srvpgm_proc(name: &str) -> bool {
    SYSTEM_SRVPGM_PROCS
        .iter()
        .any(|(n, _)| n.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_found_in_any_case() {
        let mut seen = std::collections::HashSet::new();
        for (n, s) in SYSTEM_SRVPGM_PROCS {
            assert!(seen.insert(n.to_ascii_uppercase()), "{n} twice");
            assert!(s.starts_with('Q'));
        }
        assert_eq!(system_srvpgm("SOCKET"), Some("QSOSRV1"));
        assert_eq!(system_srvpgm("socket"), Some("QSOSRV1"));
        assert_eq!(system_srvpgm("__ERRNO"), Some("QC2UTIL1"));
        assert_eq!(system_srvpgm("CEEDAYS"), Some("QLEAWI"));
        assert_eq!(system_srvpgm("_QRNX_SQRT"), Some("QRNXIE"));
        assert_eq!(system_srvpgm("NOSUCH"), None);
        assert!(is_system_srvpgm_proc("GETADDRINFO"));
    }
}
