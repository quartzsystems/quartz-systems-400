// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's numbers: the values ILE programs pass and read, exactly as
//! `QSYSINC` spells them (`SYS/SOCKET`, `NETINET/IN`, `NETINET/TCP`,
//! `SYS/POLL`, `SYS/IOCTL`, `H/FCNTL`, `H/NETDB`, `H/RESOLV`,
//! `SYS/TYPES`). Linux's values never reach a program; the `sys` layer
//! translates at the system call.

#![allow(missing_docs)]

// Address families.
pub const AF_UNSPEC: i32 = 0;
pub const AF_UNIX: i32 = 1;
pub const AF_INET: i32 = 2;
pub const AF_NS: i32 = 6;
pub const AF_INET6: i32 = 24;
pub const AF_UNIX_CCSID: i32 = 98;
pub const AF_TELEPHONY: i32 = 99;

// Socket types.
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;
pub const SOCK_RAW: i32 = 3;
pub const SOCK_SEQPACKET: i32 = 5;

// Protocols.
pub const IPPROTO_IP: i32 = 0;
pub const IPPROTO_ICMP: i32 = 1;
pub const IPPROTO_TCP: i32 = 6;
pub const IPPROTO_UDP: i32 = 17;
pub const IPPROTO_IPV6: i32 = 41;
pub const IPPROTO_ICMPV6: i32 = 58;
pub const IPPROTO_RAW: i32 = 255;

// Option levels and socket-level options.
pub const SOL_SOCKET: i32 = -1;
pub const SO_BROADCAST: i32 = 5;
pub const SO_ACCEPTECONNABORTED: i32 = 6;
pub const SO_ACCEPTEPERM: i32 = 7;
pub const SO_DEBUG: i32 = 10;
pub const SO_DONTROUTE: i32 = 15;
pub const SO_ERROR: i32 = 20;
pub const SO_KEEPALIVE: i32 = 25;
pub const SO_LINGER: i32 = 30;
pub const SO_OOBINLINE: i32 = 35;
pub const SO_RCVBUF: i32 = 40;
pub const SO_RCVLOWAT: i32 = 45;
pub const SO_RCVTIMEO: i32 = 50;
pub const SO_REUSEADDR: i32 = 55;
pub const SO_SNDBUF: i32 = 60;
pub const SO_SNDLOWAT: i32 = 65;
pub const SO_SNDTIMEO: i32 = 70;
pub const SO_TYPE: i32 = 75;
pub const SO_USELOOPBACK: i32 = 80;
pub const SO_ACCEPTCONN: i32 = 85;

// IP level options.
pub const IP_OPTIONS: i32 = 5;
pub const IP_TOS: i32 = 10;
pub const IP_TTL: i32 = 15;
pub const IP_MULTICAST_IF: i32 = 20;
pub const IP_MULTICAST_TTL: i32 = 25;
pub const IP_MULTICAST_LOOP: i32 = 30;
pub const IP_ADD_MEMBERSHIP: i32 = 35;
pub const IP_DROP_MEMBERSHIP: i32 = 40;
pub const IP_DONTFRAG: i32 = 55;
pub const IP_RECVPKTINFO: i32 = 96;
pub const IP_PKTINFO: i32 = 97;

// IPv6 level options; the shared ones take IP's numbers.
pub const IPV6_V6ONLY: i32 = 100;
pub const IPV6_RECVHOPLIMIT: i32 = 101;
pub const IPV6_UNICAST_HOPS: i32 = IP_TTL;
pub const IPV6_MULTICAST_IF: i32 = IP_MULTICAST_IF;
pub const IPV6_MULTICAST_HOPS: i32 = IP_MULTICAST_TTL;
pub const IPV6_MULTICAST_LOOP: i32 = IP_MULTICAST_LOOP;
pub const IPV6_JOIN_GROUP: i32 = IP_ADD_MEMBERSHIP;
pub const IPV6_LEAVE_GROUP: i32 = IP_DROP_MEMBERSHIP;
pub const IPV6_DONTFRAG: i32 = IP_DONTFRAG;
pub const IPV6_CHECKSUM: i32 = 105;
pub const IPV6_HOPLIMIT: i32 = 115;
pub const IPV6_RECVPKTINFO: i32 = 140;
pub const IPV6_PKTINFO: i32 = 145;
pub const IPV6_TCLASS: i32 = 190;

// TCP level options.
pub const TCP_MAXSEG: i32 = 5;
pub const TCP_NODELAY: i32 = 10;
pub const TCP_KEEPALIVE: i32 = 15;

// Message flags.
pub const MSG_DONTROUTE: i32 = 0x0001;
pub const MSG_OOB: i32 = 0x0004;
pub const MSG_PEEK: i32 = 0x0008;
pub const MSG_TRUNC: i32 = 0x0010;
pub const MSG_CTRUNC: i32 = 0x0020;
pub const MSG_WAITALL: i32 = 0x0040;
pub const MSG_EOR: i32 = 0x0080;

/// `IOV_MAX`.
pub const IOV_MAX: usize = 16;
/// `SOMAXCONN`.
pub const SOMAXCONN: i32 = 512;

// Shutdown.
pub const SHUT_RD: i32 = 0;
pub const SHUT_WR: i32 = 1;
pub const SHUT_RDWR: i32 = 2;

// Ancillary data.
pub const SCM_RIGHTS: i32 = 0x01;

// Addresses.
pub const INADDR_ANY: u32 = 0x0000_0000;
pub const INADDR_BROADCAST: u32 = 0xffff_ffff;
pub const INADDR_LOOPBACK: u32 = 0x7f00_0001;
pub const INADDR_NONE: u32 = 0xffff_ffff;
pub const INET_ADDRSTRLEN: usize = 16;
pub const INET6_ADDRSTRLEN: usize = 46;
pub const IPPORT_RESERVED: u16 = 1024;

// poll().
pub const POLLIN: i16 = 0x0001;
pub const POLLOUT: i16 = 0x0002;
pub const POLLPRI: i16 = 0x0004;
pub const POLLWRNORM: i16 = POLLOUT;
pub const POLLRDNORM: i16 = 0x0010;
pub const POLLRDBAND: i16 = 0x0020;
pub const POLLWRBAND: i16 = 0x0040;
pub const POLLHUP: i16 = 0x2000;
pub const POLLERR: i16 = 0x4000;
pub const POLLNVAL: i16 = 0x8000u16.cast_signed();
pub const POLLNORM: i16 = POLLIN;

// select(): `fd_set` is seven `unsigned int`s.
pub const FD_SETSIZE: usize = 200;
pub const FD_SET_WORDS: usize = 7;

// ioctl(): `(device << 16) | op`; device 2 is the file system.
pub const FIONREAD: i32 = 0x0002_0001;
pub const FIONBIO: i32 = 0x0002_0002;
pub const FIOCCSID: i32 = 0x0002_000d;
pub const FIOASYNC: i32 = 0x0002_0003;
pub const SIOCATMARK: i32 = 0x0001_0001;

// fcntl().
pub const F_DUPFD: i32 = 0;
pub const F_GETFD: i32 = 1;
pub const F_SETFD: i32 = 2;
pub const F_GETLK: i32 = 3;
pub const F_SETLK: i32 = 4;
pub const F_SETLKW: i32 = 5;
pub const F_GETFL: i32 = 6;
pub const F_SETFL: i32 = 7;
pub const F_GETOWN: i32 = 8;
pub const F_SETOWN: i32 = 9;
pub const FD_CLOEXEC: i32 = 0o1;
pub const O_RDONLY: i32 = 0o1;
pub const O_WRONLY: i32 = 0o2;
pub const O_RDWR: i32 = 0o4;
pub const O_NONBLOCK: i32 = 0o200;
pub const O_APPEND: i32 = 0o400;

// <netdb.h>: h_errno.
pub const HOST_NOT_FOUND: i32 = 5;
pub const NO_DATA: i32 = 10;
pub const NO_ADDRESS: i32 = NO_DATA;
pub const NO_RECOVERY: i32 = 15;
pub const TRY_AGAIN: i32 = 20;
pub const NETDB_MAX_HOST_NAME_LENGTH: usize = 255;
pub const NETDB_MAX_ARRAY_SIZE: usize = 64;
pub const NETDB_MAX_HOST_ADDR_ARRAY_SIZE: usize = 100;
pub const NETDB_MAX_ALIASES: usize = 4;
pub const NETDB_MAX_PROTO_NAME_LENGTH: usize = 32;
pub const NETDB_MAX_SERV_NAME_LENGTH: usize = 32;

// getaddrinfo().
pub const EAI_AGAIN: i32 = 1;
pub const EAI_BADFLAGS: i32 = 2;
pub const EAI_FAIL: i32 = 3;
pub const EAI_FAMILY: i32 = 4;
pub const EAI_MEMORY: i32 = 5;
pub const EAI_NONAME: i32 = 6;
pub const EAI_SERVICE: i32 = 7;
pub const EAI_SOCKTYPE: i32 = 8;
pub const EAI_SYSTEM: i32 = 9;
pub const EAI_OVERFLOW: i32 = 10;
pub const EAI_BADEXTFLAGS: i32 = 11;
pub const AI_PASSIVE: i32 = 0x0000_0001;
pub const AI_CANONNAME: i32 = 0x0000_0002;
pub const AI_NUMERICHOST: i32 = 0x0000_0004;
pub const AI_NUMERICSERV: i32 = 0x0000_0008;
pub const AI_V4MAPPED: i32 = 0x0000_0010;
pub const AI_ALL: i32 = 0x0000_0020;
pub const AI_ADDRCONFIG: i32 = 0x0000_0040;
pub const AI_EXTFLAGS: i32 = 0x0000_0080;
pub const AI_MASK: i32 = 0x0000_00ff;
pub const NI_NOFQDN: i32 = 0x0000_0001;
pub const NI_NUMERICHOST: i32 = 0x0000_0002;
pub const NI_NAMEREQD: i32 = 0x0000_0004;
pub const NI_NUMERICSERV: i32 = 0x0000_0008;
pub const NI_NUMERICSCOPE: i32 = 0x0000_0010;
pub const NI_DGRAM: i32 = 0x0000_0020;
pub const NI_MASK: i32 = 0x0000_003f;
/// `NI_MAXHOST`, the host buffer `getnameinfo` callers size.
pub const NI_MAXHOST: usize = 1025;
/// `NI_MAXSERV`.
pub const NI_MAXSERV: usize = 32;

// <resolv.h>.
pub const RES_AAONLY: i32 = 0x0001;
pub const RES_DEFNAMES: i32 = 0x0002;
pub const RES_DNSRCH: i32 = 0x0004;
pub const RES_IGNTC: i32 = 0x0008;
pub const RES_INIT: i32 = 0x0010;
pub const RES_RECURSE: i32 = 0x0020;
pub const RES_STAYOPEN: i32 = 0x0040;
pub const RES_USEVC: i32 = 0x0080;
pub const RES_DEFAULT: i32 = RES_RECURSE | RES_DEFNAMES | RES_DNSRCH;
pub const MAXDNSRCH: usize = 6;
pub const MAXNS: usize = 3;
pub const MAXDNAME: usize = 256;
/// `NS_PACKETSZ`: the largest UDP answer.
pub const NS_PACKETSZ: usize = 512;
/// `PACKETSZ` alias.
pub const PACKETSZ: usize = NS_PACKETSZ;

// DNS classes and types the resolver speaks.
pub const C_IN: u16 = 1;
pub const T_A: u16 = 1;
pub const T_NS: u16 = 2;
pub const T_CNAME: u16 = 5;
pub const T_SOA: u16 = 6;
pub const T_PTR: u16 = 12;
pub const T_MX: u16 = 15;
pub const T_TXT: u16 = 16;
pub const T_AAAA: u16 = 28;
pub const T_SRV: u16 = 33;
pub const T_ANY: u16 = 255;

/// Every number above by name, for the spec test and the docs table.
#[allow(clippy::cast_possible_wrap, clippy::cast_lossless)]
pub const ALL: &[(&str, i64)] = &[
    ("AF_UNSPEC", AF_UNSPEC as i64),
    ("AF_UNIX", AF_UNIX as i64),
    ("AF_INET", AF_INET as i64),
    ("AF_NS", AF_NS as i64),
    ("AF_INET6", AF_INET6 as i64),
    ("AF_UNIX_CCSID", AF_UNIX_CCSID as i64),
    ("AF_TELEPHONY", AF_TELEPHONY as i64),
    ("SOCK_STREAM", SOCK_STREAM as i64),
    ("SOCK_DGRAM", SOCK_DGRAM as i64),
    ("SOCK_RAW", SOCK_RAW as i64),
    ("SOCK_SEQPACKET", SOCK_SEQPACKET as i64),
    ("IPPROTO_IP", IPPROTO_IP as i64),
    ("IPPROTO_ICMP", IPPROTO_ICMP as i64),
    ("IPPROTO_TCP", IPPROTO_TCP as i64),
    ("IPPROTO_UDP", IPPROTO_UDP as i64),
    ("IPPROTO_IPV6", IPPROTO_IPV6 as i64),
    ("IPPROTO_ICMPV6", IPPROTO_ICMPV6 as i64),
    ("IPPROTO_RAW", IPPROTO_RAW as i64),
    ("SOL_SOCKET", SOL_SOCKET as i64),
    ("SO_BROADCAST", SO_BROADCAST as i64),
    ("SO_ACCEPTECONNABORTED", SO_ACCEPTECONNABORTED as i64),
    ("SO_ACCEPTEPERM", SO_ACCEPTEPERM as i64),
    ("SO_DEBUG", SO_DEBUG as i64),
    ("SO_DONTROUTE", SO_DONTROUTE as i64),
    ("SO_ERROR", SO_ERROR as i64),
    ("SO_KEEPALIVE", SO_KEEPALIVE as i64),
    ("SO_LINGER", SO_LINGER as i64),
    ("SO_OOBINLINE", SO_OOBINLINE as i64),
    ("SO_RCVBUF", SO_RCVBUF as i64),
    ("SO_RCVLOWAT", SO_RCVLOWAT as i64),
    ("SO_RCVTIMEO", SO_RCVTIMEO as i64),
    ("SO_REUSEADDR", SO_REUSEADDR as i64),
    ("SO_SNDBUF", SO_SNDBUF as i64),
    ("SO_SNDLOWAT", SO_SNDLOWAT as i64),
    ("SO_SNDTIMEO", SO_SNDTIMEO as i64),
    ("SO_TYPE", SO_TYPE as i64),
    ("SO_USELOOPBACK", SO_USELOOPBACK as i64),
    ("SO_ACCEPTCONN", SO_ACCEPTCONN as i64),
    ("IP_OPTIONS", IP_OPTIONS as i64),
    ("IP_TOS", IP_TOS as i64),
    ("IP_TTL", IP_TTL as i64),
    ("IP_MULTICAST_IF", IP_MULTICAST_IF as i64),
    ("IP_MULTICAST_TTL", IP_MULTICAST_TTL as i64),
    ("IP_MULTICAST_LOOP", IP_MULTICAST_LOOP as i64),
    ("IP_ADD_MEMBERSHIP", IP_ADD_MEMBERSHIP as i64),
    ("IP_DROP_MEMBERSHIP", IP_DROP_MEMBERSHIP as i64),
    ("IP_DONTFRAG", IP_DONTFRAG as i64),
    ("IP_RECVPKTINFO", IP_RECVPKTINFO as i64),
    ("IP_PKTINFO", IP_PKTINFO as i64),
    ("IPV6_V6ONLY", IPV6_V6ONLY as i64),
    ("IPV6_RECVHOPLIMIT", IPV6_RECVHOPLIMIT as i64),
    ("IPV6_UNICAST_HOPS", IPV6_UNICAST_HOPS as i64),
    ("IPV6_MULTICAST_IF", IPV6_MULTICAST_IF as i64),
    ("IPV6_MULTICAST_HOPS", IPV6_MULTICAST_HOPS as i64),
    ("IPV6_MULTICAST_LOOP", IPV6_MULTICAST_LOOP as i64),
    ("IPV6_JOIN_GROUP", IPV6_JOIN_GROUP as i64),
    ("IPV6_LEAVE_GROUP", IPV6_LEAVE_GROUP as i64),
    ("IPV6_DONTFRAG", IPV6_DONTFRAG as i64),
    ("IPV6_CHECKSUM", IPV6_CHECKSUM as i64),
    ("IPV6_HOPLIMIT", IPV6_HOPLIMIT as i64),
    ("IPV6_RECVPKTINFO", IPV6_RECVPKTINFO as i64),
    ("IPV6_PKTINFO", IPV6_PKTINFO as i64),
    ("IPV6_TCLASS", IPV6_TCLASS as i64),
    ("TCP_MAXSEG", TCP_MAXSEG as i64),
    ("TCP_NODELAY", TCP_NODELAY as i64),
    ("TCP_KEEPALIVE", TCP_KEEPALIVE as i64),
    ("MSG_DONTROUTE", MSG_DONTROUTE as i64),
    ("MSG_OOB", MSG_OOB as i64),
    ("MSG_PEEK", MSG_PEEK as i64),
    ("MSG_TRUNC", MSG_TRUNC as i64),
    ("MSG_CTRUNC", MSG_CTRUNC as i64),
    ("MSG_WAITALL", MSG_WAITALL as i64),
    ("MSG_EOR", MSG_EOR as i64),
    ("IOV_MAX", IOV_MAX as i64),
    ("SOMAXCONN", SOMAXCONN as i64),
    ("SHUT_RD", SHUT_RD as i64),
    ("SHUT_WR", SHUT_WR as i64),
    ("SHUT_RDWR", SHUT_RDWR as i64),
    ("SCM_RIGHTS", SCM_RIGHTS as i64),
    ("INADDR_ANY", INADDR_ANY as i64),
    ("INADDR_BROADCAST", INADDR_BROADCAST as i64),
    ("INADDR_LOOPBACK", INADDR_LOOPBACK as i64),
    ("INADDR_NONE", INADDR_NONE as i64),
    ("INET_ADDRSTRLEN", INET_ADDRSTRLEN as i64),
    ("INET6_ADDRSTRLEN", INET6_ADDRSTRLEN as i64),
    ("IPPORT_RESERVED", IPPORT_RESERVED as i64),
    ("POLLIN", POLLIN as i64),
    ("POLLOUT", POLLOUT as i64),
    ("POLLPRI", POLLPRI as i64),
    ("POLLWRNORM", POLLWRNORM as i64),
    ("POLLRDNORM", POLLRDNORM as i64),
    ("POLLRDBAND", POLLRDBAND as i64),
    ("POLLWRBAND", POLLWRBAND as i64),
    ("POLLHUP", POLLHUP as i64),
    ("POLLERR", POLLERR as i64),
    ("POLLNVAL", POLLNVAL as i64),
    ("POLLNORM", POLLNORM as i64),
    ("FD_SETSIZE", FD_SETSIZE as i64),
    ("FD_SET_WORDS", FD_SET_WORDS as i64),
    ("FIONREAD", FIONREAD as i64),
    ("FIONBIO", FIONBIO as i64),
    ("FIOCCSID", FIOCCSID as i64),
    ("FIOASYNC", FIOASYNC as i64),
    ("SIOCATMARK", SIOCATMARK as i64),
    ("F_DUPFD", F_DUPFD as i64),
    ("F_GETFD", F_GETFD as i64),
    ("F_SETFD", F_SETFD as i64),
    ("F_GETLK", F_GETLK as i64),
    ("F_SETLK", F_SETLK as i64),
    ("F_SETLKW", F_SETLKW as i64),
    ("F_GETFL", F_GETFL as i64),
    ("F_SETFL", F_SETFL as i64),
    ("F_GETOWN", F_GETOWN as i64),
    ("F_SETOWN", F_SETOWN as i64),
    ("FD_CLOEXEC", FD_CLOEXEC as i64),
    ("O_RDONLY", O_RDONLY as i64),
    ("O_WRONLY", O_WRONLY as i64),
    ("O_RDWR", O_RDWR as i64),
    ("O_NONBLOCK", O_NONBLOCK as i64),
    ("O_APPEND", O_APPEND as i64),
    ("HOST_NOT_FOUND", HOST_NOT_FOUND as i64),
    ("NO_DATA", NO_DATA as i64),
    ("NO_ADDRESS", NO_ADDRESS as i64),
    ("NO_RECOVERY", NO_RECOVERY as i64),
    ("TRY_AGAIN", TRY_AGAIN as i64),
    ("NETDB_MAX_HOST_NAME_LENGTH", NETDB_MAX_HOST_NAME_LENGTH as i64),
    ("NETDB_MAX_ARRAY_SIZE", NETDB_MAX_ARRAY_SIZE as i64),
    ("NETDB_MAX_HOST_ADDR_ARRAY_SIZE", NETDB_MAX_HOST_ADDR_ARRAY_SIZE as i64),
    ("NETDB_MAX_ALIASES", NETDB_MAX_ALIASES as i64),
    ("NETDB_MAX_PROTO_NAME_LENGTH", NETDB_MAX_PROTO_NAME_LENGTH as i64),
    ("NETDB_MAX_SERV_NAME_LENGTH", NETDB_MAX_SERV_NAME_LENGTH as i64),
    ("EAI_AGAIN", EAI_AGAIN as i64),
    ("EAI_BADFLAGS", EAI_BADFLAGS as i64),
    ("EAI_FAIL", EAI_FAIL as i64),
    ("EAI_FAMILY", EAI_FAMILY as i64),
    ("EAI_MEMORY", EAI_MEMORY as i64),
    ("EAI_NONAME", EAI_NONAME as i64),
    ("EAI_SERVICE", EAI_SERVICE as i64),
    ("EAI_SOCKTYPE", EAI_SOCKTYPE as i64),
    ("EAI_SYSTEM", EAI_SYSTEM as i64),
    ("EAI_OVERFLOW", EAI_OVERFLOW as i64),
    ("EAI_BADEXTFLAGS", EAI_BADEXTFLAGS as i64),
    ("AI_PASSIVE", AI_PASSIVE as i64),
    ("AI_CANONNAME", AI_CANONNAME as i64),
    ("AI_NUMERICHOST", AI_NUMERICHOST as i64),
    ("AI_NUMERICSERV", AI_NUMERICSERV as i64),
    ("AI_V4MAPPED", AI_V4MAPPED as i64),
    ("AI_ALL", AI_ALL as i64),
    ("AI_ADDRCONFIG", AI_ADDRCONFIG as i64),
    ("AI_EXTFLAGS", AI_EXTFLAGS as i64),
    ("AI_MASK", AI_MASK as i64),
    ("NI_NOFQDN", NI_NOFQDN as i64),
    ("NI_NUMERICHOST", NI_NUMERICHOST as i64),
    ("NI_NAMEREQD", NI_NAMEREQD as i64),
    ("NI_NUMERICSERV", NI_NUMERICSERV as i64),
    ("NI_NUMERICSCOPE", NI_NUMERICSCOPE as i64),
    ("NI_DGRAM", NI_DGRAM as i64),
    ("NI_MASK", NI_MASK as i64),
    ("NI_MAXHOST", NI_MAXHOST as i64),
    ("NI_MAXSERV", NI_MAXSERV as i64),
    ("RES_AAONLY", RES_AAONLY as i64),
    ("RES_DEFNAMES", RES_DEFNAMES as i64),
    ("RES_DNSRCH", RES_DNSRCH as i64),
    ("RES_IGNTC", RES_IGNTC as i64),
    ("RES_INIT", RES_INIT as i64),
    ("RES_RECURSE", RES_RECURSE as i64),
    ("RES_STAYOPEN", RES_STAYOPEN as i64),
    ("RES_USEVC", RES_USEVC as i64),
    ("RES_DEFAULT", RES_DEFAULT as i64),
    ("MAXDNSRCH", MAXDNSRCH as i64),
    ("MAXNS", MAXNS as i64),
    ("MAXDNAME", MAXDNAME as i64),
    ("NS_PACKETSZ", NS_PACKETSZ as i64),
    ("PACKETSZ", PACKETSZ as i64),
    ("C_IN", C_IN as i64),
    ("T_A", T_A as i64),
    ("T_NS", T_NS as i64),
    ("T_CNAME", T_CNAME as i64),
    ("T_SOA", T_SOA as i64),
    ("T_PTR", T_PTR as i64),
    ("T_MX", T_MX as i64),
    ("T_TXT", T_TXT as i64),
    ("T_AAAA", T_AAAA as i64),
    ("T_SRV", T_SRV as i64),
    ("T_ANY", T_ANY as i64),
];
