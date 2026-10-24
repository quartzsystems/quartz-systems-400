// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! rtnetlink numbers (`linux/rtnetlink.h`, `if_link.h`, `if_addr.h`,
//! `neighbour.h`).

#![allow(missing_docs)]

pub const RTM_NEWLINK: u16 = 16;
pub const RTM_DELLINK: u16 = 17;
pub const RTM_GETLINK: u16 = 18;
pub const RTM_NEWADDR: u16 = 20;
pub const RTM_DELADDR: u16 = 21;
pub const RTM_GETADDR: u16 = 22;
pub const RTM_NEWROUTE: u16 = 24;
pub const RTM_DELROUTE: u16 = 25;
pub const RTM_GETROUTE: u16 = 26;
pub const RTM_NEWNEIGH: u16 = 28;
pub const RTM_DELNEIGH: u16 = 29;
pub const RTM_GETNEIGH: u16 = 30;

/// Multicast group bit for link changes (`RTMGRP_LINK`, group 1).
pub const RTMGRP_LINK: u32 = 1;
/// `RTNLGRP_LINK`.
pub const RTNLGRP_LINK: u32 = 1;

/// Size of `ifinfomsg`.
pub const IFINFOMSG_LEN: usize = 16;
/// Size of `ifaddrmsg`.
pub const IFADDRMSG_LEN: usize = 8;
/// Size of `rtmsg`.
pub const RTMSG_LEN: usize = 12;
/// Size of `ndmsg`.
pub const NDMSG_LEN: usize = 12;

pub const IFF_UP: u32 = 0x1;
pub const IFF_BROADCAST: u32 = 0x2;
pub const IFF_LOOPBACK: u32 = 0x8;
pub const IFF_POINTOPOINT: u32 = 0x10;
pub const IFF_RUNNING: u32 = 0x40;
pub const IFF_NOARP: u32 = 0x80;
pub const IFF_PROMISC: u32 = 0x100;
pub const IFF_MASTER: u32 = 0x400;
pub const IFF_SLAVE: u32 = 0x800;
pub const IFF_MULTICAST: u32 = 0x1000;
pub const IFF_LOWER_UP: u32 = 0x10000;

pub const IFLA_ADDRESS: u16 = 1;
pub const IFLA_BROADCAST: u16 = 2;
pub const IFLA_IFNAME: u16 = 3;
pub const IFLA_MTU: u16 = 4;
pub const IFLA_LINK: u16 = 5;
pub const IFLA_MASTER: u16 = 10;
pub const IFLA_TXQLEN: u16 = 13;
pub const IFLA_OPERSTATE: u16 = 16;
pub const IFLA_LINKINFO: u16 = 18;
pub const IFLA_IFALIAS: u16 = 20;
pub const IFLA_STATS64: u16 = 23;
pub const IFLA_CARRIER: u16 = 33;
pub const IFLA_MIN_MTU: u16 = 50;
pub const IFLA_MAX_MTU: u16 = 51;
pub const IFLA_PERM_ADDRESS: u16 = 54;

pub const IFLA_INFO_KIND: u16 = 1;
pub const IFLA_INFO_DATA: u16 = 2;
pub const IFLA_INFO_SLAVE_KIND: u16 = 4;

pub const IFLA_VLAN_ID: u16 = 1;
pub const IFLA_VLAN_PROTOCOL: u16 = 5;
/// `ETH_P_8021Q`.
pub const ETH_P_8021Q: u16 = 0x8100;

pub const IFLA_BOND_MODE: u16 = 1;
pub const IFLA_BOND_MIIMON: u16 = 3;
pub const IFLA_BOND_AD_LACP_RATE: u16 = 13;
pub const IFLA_BOND_XMIT_HASH_POLICY: u16 = 14;

pub const IFA_ADDRESS: u16 = 1;
pub const IFA_LOCAL: u16 = 2;
pub const IFA_LABEL: u16 = 3;
pub const IFA_BROADCAST: u16 = 4;
pub const IFA_CACHEINFO: u16 = 6;
pub const IFA_FLAGS: u16 = 8;

pub const IFA_F_SECONDARY: u32 = 0x01;
pub const IFA_F_NODAD: u32 = 0x02;
pub const IFA_F_OPTIMISTIC: u32 = 0x04;
pub const IFA_F_DADFAILED: u32 = 0x08;
pub const IFA_F_HOMEADDRESS: u32 = 0x10;
pub const IFA_F_DEPRECATED: u32 = 0x20;
pub const IFA_F_TENTATIVE: u32 = 0x40;
pub const IFA_F_PERMANENT: u32 = 0x80;
pub const IFA_F_MANAGETEMPADDR: u32 = 0x100;
pub const IFA_F_NOPREFIXROUTE: u32 = 0x200;

pub const RT_SCOPE_UNIVERSE: u8 = 0;
pub const RT_SCOPE_SITE: u8 = 200;
pub const RT_SCOPE_LINK: u8 = 253;
pub const RT_SCOPE_HOST: u8 = 254;

pub const RTA_DST: u16 = 1;
pub const RTA_SRC: u16 = 2;
pub const RTA_OIF: u16 = 4;
pub const RTA_GATEWAY: u16 = 5;
pub const RTA_PRIORITY: u16 = 6;
pub const RTA_PREFSRC: u16 = 7;
pub const RTA_METRICS: u16 = 8;
pub const RTA_TABLE: u16 = 15;
pub const RTAX_MTU: u16 = 2;

pub const RT_TABLE_UNSPEC: u32 = 0;
pub const RT_TABLE_MAIN: u32 = 254;
pub const RT_TABLE_LOCAL: u32 = 255;

pub const RTPROT_KERNEL: u8 = 2;
pub const RTPROT_BOOT: u8 = 3;
pub const RTPROT_STATIC: u8 = 4;
pub const RTPROT_RA: u8 = 9;
/// Routes QS/400 configures (`ADDTCPRTE`) carry this protocol so they
/// can be told from kernel and boot routes. Unassigned in iproute2.
pub const RTPROT_QS400: u8 = 81;

pub const RTN_UNICAST: u8 = 1;
pub const RTN_LOCAL: u8 = 2;
pub const RTN_BROADCAST: u8 = 3;
pub const RTN_MULTICAST: u8 = 5;
pub const RTN_BLACKHOLE: u8 = 6;
pub const RTN_UNREACHABLE: u8 = 7;

pub const NDA_DST: u16 = 1;
pub const NDA_LLADDR: u16 = 2;

pub const NUD_INCOMPLETE: u16 = 0x01;
pub const NUD_REACHABLE: u16 = 0x02;
pub const NUD_STALE: u16 = 0x04;
pub const NUD_DELAY: u16 = 0x08;
pub const NUD_PROBE: u16 = 0x10;
pub const NUD_FAILED: u16 = 0x20;
pub const NUD_NOARP: u16 = 0x40;
pub const NUD_PERMANENT: u16 = 0x80;
