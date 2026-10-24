// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parts of `struct tcp_info` NETSTAT shows. Offsets are fixed by the
//! kernel ABI; later kernels only append. A field past the end of what
//! the kernel sent reads as 0.

use crate::codec::{ne_u32, ne_u64};

/// TCP counters for one connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TcpInfo {
    /// `tcpi_state`.
    pub state: u8,
    /// Retransmission timeout, microseconds.
    pub rto: u32,
    /// Sender MSS.
    pub snd_mss: u32,
    /// Receiver MSS.
    pub rcv_mss: u32,
    /// Milliseconds since data was last sent.
    pub last_data_sent: u32,
    /// Milliseconds since data was last received.
    pub last_data_recv: u32,
    /// Milliseconds since an ACK was last received.
    pub last_ack_recv: u32,
    /// Path MTU.
    pub pmtu: u32,
    /// Smoothed round-trip time, microseconds.
    pub rtt: u32,
    /// Round-trip variance, microseconds.
    pub rttvar: u32,
    /// Congestion window, segments.
    pub snd_cwnd: u32,
    /// Retransmitted segments over the life of the connection.
    pub total_retrans: u32,
    /// Bytes acknowledged by the peer.
    pub bytes_acked: u64,
    /// Bytes received.
    pub bytes_received: u64,
    /// Segments sent.
    pub segs_out: u32,
    /// Segments received.
    pub segs_in: u32,
    /// Bytes sent, retransmissions included.
    pub bytes_sent: u64,
}

impl TcpInfo {
    /// Parse the `INET_DIAG_INFO` payload. `None` if shorter than the
    /// original 104-byte structure.
    pub fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 104 {
            return None;
        }
        let u = |off| ne_u32(b, off).unwrap_or(0);
        let q = |off| ne_u64(b, off).unwrap_or(0);
        Some(Self {
            state: b[0],
            rto: u(8),
            snd_mss: u(16),
            rcv_mss: u(20),
            last_data_sent: u(44),
            last_data_recv: u(52),
            last_ack_recv: u(56),
            pmtu: u(60),
            rtt: u(68),
            rttvar: u(72),
            snd_cwnd: u(80),
            total_retrans: u(100),
            bytes_acked: q(120),
            bytes_received: q(128),
            segs_out: u(136),
            segs_in: u(140),
            bytes_sent: q(200),
        })
    }
}
