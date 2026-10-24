// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NETLINK_SOCK_DIAG`: the kernel's TCP and UDP sockets with their
//! owners' inodes and uids, TCP counters from `tcp_info`, and
//! `SOCK_DESTROY` (needs `CONFIG_INET_DIAG_DESTROY`; without it the
//! kernel answers `EOPNOTSUPP`).

mod tcpinfo;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

pub use tcpinfo::TcpInfo;

use crate::codec::{Attrs, Builder, NLM_F_DUMP, NLM_F_REQUEST, be_u16, ne_u32};
use crate::{Bus, Error, Family, Result, Socket};

/// `SOCK_DIAG_BY_FAMILY`.
pub const SOCK_DIAG_BY_FAMILY: u16 = 20;
/// `SOCK_DESTROY`.
pub const SOCK_DESTROY: u16 = 21;
/// `INET_DIAG_INFO`: the `tcp_info` attribute.
pub const INET_DIAG_INFO: u16 = 2;
/// Size of `inet_diag_req_v2`.
pub const REQ_LEN: usize = 56;
/// Size of `inet_diag_msg`.
pub const MSG_LEN: usize = 72;
/// Every TCP state.
pub const ALL_STATES: u32 = u32::MAX;

/// Transport protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Proto {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

impl Proto {
    /// `IPPROTO_*`.
    pub fn number(self) -> u8 {
        match self {
            Proto::Tcp => 6,
            Proto::Udp => 17,
        }
    }
}

/// `TCP_*` states as `inet_diag` reports them. UDP sockets report
/// [`TcpState::Close`] when unconnected and `Established` when connected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpState {
    /// `ESTABLISHED`.
    Established,
    /// `SYN_SENT`.
    SynSent,
    /// `SYN_RECV`.
    SynRecv,
    /// `FIN_WAIT1`.
    FinWait1,
    /// `FIN_WAIT2`.
    FinWait2,
    /// `TIME_WAIT`.
    TimeWait,
    /// `CLOSE`.
    Close,
    /// `CLOSE_WAIT`.
    CloseWait,
    /// `LAST_ACK`.
    LastAck,
    /// `LISTEN`.
    Listen,
    /// `CLOSING`.
    Closing,
    /// `NEW_SYN_RECV`.
    NewSynRecv,
    /// Anything else.
    Other(u8),
}

impl TcpState {
    /// From the kernel's number.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => TcpState::Established,
            2 => TcpState::SynSent,
            3 => TcpState::SynRecv,
            4 => TcpState::FinWait1,
            5 => TcpState::FinWait2,
            6 => TcpState::TimeWait,
            7 => TcpState::Close,
            8 => TcpState::CloseWait,
            9 => TcpState::LastAck,
            10 => TcpState::Listen,
            11 => TcpState::Closing,
            12 => TcpState::NewSynRecv,
            n => TcpState::Other(n),
        }
    }

    /// The bit for this state in a request's state mask.
    pub fn bit(self) -> u32 {
        let n = match self {
            TcpState::Established => 1,
            TcpState::SynSent => 2,
            TcpState::SynRecv => 3,
            TcpState::FinWait1 => 4,
            TcpState::FinWait2 => 5,
            TcpState::TimeWait => 6,
            TcpState::Close => 7,
            TcpState::CloseWait => 8,
            TcpState::LastAck => 9,
            TcpState::Listen => 10,
            TcpState::Closing => 11,
            TcpState::NewSynRecv => 12,
            TcpState::Other(n) => u32::from(n),
        };
        1u32.checked_shl(n).unwrap_or(0)
    }
}

/// `inet_diag_sockid`: what identifies a socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SockId {
    /// Local address and port.
    pub local: SocketAddr,
    /// Remote address and port (unspecified for listeners).
    pub remote: SocketAddr,
    /// Bound interface (0 none).
    pub ifindex: u32,
    /// Kernel cookie.
    pub cookie: u64,
}

/// One socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InetSock {
    /// Family.
    pub family: Family,
    /// Protocol.
    pub proto: Proto,
    /// State.
    pub state: TcpState,
    /// Identity.
    pub id: SockId,
    /// Pending timer (`0` none, `1` retransmit, `2` keepalive, ...).
    pub timer: u8,
    /// Retransmits of the timer.
    pub retrans: u8,
    /// Milliseconds to the timer.
    pub expires: u32,
    /// Receive queue bytes (listeners: accept backlog).
    pub rqueue: u32,
    /// Send queue bytes (listeners: backlog limit).
    pub wqueue: u32,
    /// Owner uid.
    pub uid: u32,
    /// Socket inode (0 for time-wait and request sockets).
    pub inode: u32,
    /// TCP counters, when asked for and available.
    pub info: Option<TcpInfo>,
}

fn sockid_bytes(family: Family, id: &SockId) -> [u8; 48] {
    let mut b = [0u8; 48];
    b[0..2].copy_from_slice(&id.local.port().to_be_bytes());
    b[2..4].copy_from_slice(&id.remote.port().to_be_bytes());
    put_ip(&mut b[4..20], family, id.local.ip());
    put_ip(&mut b[20..36], family, id.remote.ip());
    b[36..40].copy_from_slice(&id.ifindex.to_ne_bytes());
    let lo = u32::try_from(id.cookie & 0xffff_ffff).unwrap_or(0);
    let hi = u32::try_from(id.cookie >> 32).unwrap_or(0);
    b[40..44].copy_from_slice(&lo.to_ne_bytes());
    b[44..48].copy_from_slice(&hi.to_ne_bytes());
    b
}

fn put_ip(dst: &mut [u8], family: Family, ip: IpAddr) {
    match (family, ip) {
        (Family::V4, IpAddr::V4(a)) => dst[..4].copy_from_slice(&a.octets()),
        (Family::V6, IpAddr::V6(a)) => dst.copy_from_slice(&a.octets()),
        (Family::V6, IpAddr::V4(a)) => dst.copy_from_slice(&a.to_ipv6_mapped().octets()),
        (Family::V4, IpAddr::V6(_)) => {}
    }
}

fn get_ip(src: &[u8], family: Family) -> IpAddr {
    match family {
        Family::V4 => IpAddr::V4(Ipv4Addr::new(src[0], src[1], src[2], src[3])),
        Family::V6 => IpAddr::V6(Ipv6Addr::from(
            <[u8; 16]>::try_from(&src[..16]).unwrap_or([0; 16]),
        )),
    }
}

fn req_bytes(family: Family, proto: Proto, states: u32, info: bool, id: &SockId) -> [u8; REQ_LEN] {
    let mut b = [0u8; REQ_LEN];
    b[0] = family.af();
    b[1] = proto.number();
    // idiag_ext: bit (INET_DIAG_INFO - 1).
    b[2] = if info { 1 << (INET_DIAG_INFO - 1) } else { 0 };
    b[4..8].copy_from_slice(&states.to_ne_bytes());
    b[8..56].copy_from_slice(&sockid_bytes(family, id));
    b
}

/// An all-zero id of `family`.
fn any_id(family: Family) -> SockId {
    let ip = match family {
        Family::V4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        Family::V6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    SockId {
        local: SocketAddr::new(ip, 0),
        remote: SocketAddr::new(ip, 0),
        ifindex: 0,
        cookie: 0,
    }
}

/// Dump the sockets of `family`/`proto` in `states` (a mask of
/// [`TcpState::bit`]), with `tcp_info` if `info`.
pub fn dump_request(family: Family, proto: Proto, states: u32, info: bool) -> Builder {
    let mut b = Builder::new(SOCK_DIAG_BY_FAMILY, NLM_F_REQUEST | NLM_F_DUMP);
    b.bytes(&req_bytes(family, proto, states, info, &any_id(family)));
    b
}

/// Destroy one socket (identified by its full id including the cookie).
pub fn destroy_request(s: &InetSock) -> Builder {
    let mut b = Builder::new(SOCK_DESTROY, NLM_F_REQUEST);
    b.bytes(&req_bytes(s.family, s.proto, ALL_STATES, false, &s.id));
    b
}

/// Parse an `inet_diag_msg` reply for a request on `proto`.
pub fn parse(payload: &[u8], proto: Proto) -> Result<Option<InetSock>> {
    if payload.len() < MSG_LEN {
        return Err(Error::Malformed("inet_diag_msg"));
    }
    let Some(family) = Family::from_af(payload[0]) else {
        return Ok(None);
    };
    let id = &payload[4..52];
    let u = |off| ne_u32(payload, off).unwrap_or(0);
    let local = SocketAddr::new(get_ip(&id[4..20], family), be_u16(id, 0).unwrap_or(0));
    let remote = SocketAddr::new(get_ip(&id[20..36], family), be_u16(id, 2).unwrap_or(0));
    let cookie = u64::from(ne_u32(id, 40).unwrap_or(0)) | (u64::from(ne_u32(id, 44).unwrap_or(0)) << 32);
    let info = Attrs::new(&payload[MSG_LEN..])
        .get(INET_DIAG_INFO)
        .filter(|_| proto == Proto::Tcp)
        .and_then(|a| TcpInfo::parse(a.data));
    Ok(Some(InetSock {
        family,
        proto,
        state: TcpState::from_u8(payload[1]),
        id: SockId {
            local,
            remote,
            ifindex: ne_u32(id, 36).unwrap_or(0),
            cookie,
        },
        timer: payload[2],
        retrans: payload[3],
        expires: u(52),
        rqueue: u(56),
        wqueue: u(60),
        uid: u(64),
        inode: u(68),
        info,
    }))
}

/// A `sock_diag` client.
pub struct Diag {
    sock: Socket,
}

impl Diag {
    /// Open a request socket.
    pub fn open() -> Result<Self> {
        Ok(Self {
            sock: Socket::open(Bus::SockDiag)?,
        })
    }

    /// Sockets of one family and protocol, every state, with `tcp_info`.
    pub fn sockets(&mut self, family: Family, proto: Proto) -> Result<Vec<InetSock>> {
        self.sockets_in(family, proto, ALL_STATES)
    }

    /// Sockets in the given states.
    pub fn sockets_in(&mut self, family: Family, proto: Proto, states: u32) -> Result<Vec<InetSock>> {
        let req = dump_request(family, proto, states, proto == Proto::Tcp);
        self.sock.dump(&req, |m| {
            if m.kind == SOCK_DIAG_BY_FAMILY {
                parse(m.payload, proto)
            } else {
                Ok(None)
            }
        })
    }

    /// TCP and UDP, IPv4 and IPv6.
    pub fn all(&mut self) -> Result<Vec<InetSock>> {
        let mut out = Vec::new();
        for proto in [Proto::Tcp, Proto::Udp] {
            for family in [Family::V4, Family::V6] {
                out.extend(self.sockets(family, proto)?);
            }
        }
        Ok(out)
    }

    /// Close a socket under its owner. Errors with errno `EOPNOTSUPP`
    /// when the kernel lacks `CONFIG_INET_DIAG_DESTROY`, `ENOENT` when
    /// the socket is gone.
    pub fn destroy(&mut self, s: &InetSock) -> Result<()> {
        self.sock.ack(&destroy_request(s))
    }
}
