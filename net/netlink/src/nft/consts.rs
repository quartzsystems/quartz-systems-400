// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfnetlink` and `nf_tables` numbers (`linux/netfilter/nfnetlink.h`,
//! `nf_tables.h`). `nf_tables` attribute integers are big-endian.

#![allow(missing_docs)]

pub const NFNL_SUBSYS_NFTABLES: u16 = 10;
pub const NFNL_MSG_BATCH_BEGIN: u16 = 0x10;
pub const NFNL_MSG_BATCH_END: u16 = 0x11;
pub const NFNETLINK_V0: u8 = 0;

/// `NFPROTO_INET`: the dual-stack family.
pub const NFPROTO_INET: u8 = 1;

pub const NFT_MSG_NEWTABLE: u16 = 0;
pub const NFT_MSG_GETTABLE: u16 = 1;
pub const NFT_MSG_DELTABLE: u16 = 2;
pub const NFT_MSG_NEWCHAIN: u16 = 3;
pub const NFT_MSG_DELCHAIN: u16 = 5;
pub const NFT_MSG_NEWRULE: u16 = 6;
pub const NFT_MSG_GETRULE: u16 = 7;
pub const NFT_MSG_DELRULE: u16 = 8;

pub const NFTA_TABLE_NAME: u16 = 1;
pub const NFTA_TABLE_FLAGS: u16 = 2;

pub const NFTA_CHAIN_TABLE: u16 = 1;
pub const NFTA_CHAIN_NAME: u16 = 3;
pub const NFTA_CHAIN_HOOK: u16 = 4;
pub const NFTA_CHAIN_POLICY: u16 = 5;
pub const NFTA_CHAIN_TYPE: u16 = 7;
pub const NFTA_HOOK_HOOKNUM: u16 = 1;
pub const NFTA_HOOK_PRIORITY: u16 = 2;

pub const NFTA_RULE_TABLE: u16 = 1;
pub const NFTA_RULE_CHAIN: u16 = 2;
pub const NFTA_RULE_HANDLE: u16 = 3;
pub const NFTA_RULE_EXPRESSIONS: u16 = 4;
pub const NFTA_RULE_USERDATA: u16 = 7;

pub const NFTA_LIST_ELEM: u16 = 1;
pub const NFTA_EXPR_NAME: u16 = 1;
pub const NFTA_EXPR_DATA: u16 = 2;

pub const NFTA_DATA_VALUE: u16 = 1;
pub const NFTA_DATA_VERDICT: u16 = 2;
pub const NFTA_VERDICT_CODE: u16 = 1;

pub const NFTA_META_DREG: u16 = 1;
pub const NFTA_META_KEY: u16 = 2;
pub const NFT_META_L4PROTO: u32 = 16;

pub const NFTA_PAYLOAD_DREG: u16 = 1;
pub const NFTA_PAYLOAD_BASE: u16 = 2;
pub const NFTA_PAYLOAD_OFFSET: u16 = 3;
pub const NFTA_PAYLOAD_LEN: u16 = 4;
pub const NFT_PAYLOAD_TRANSPORT_HEADER: u32 = 2;

pub const NFTA_CMP_SREG: u16 = 1;
pub const NFTA_CMP_OP: u16 = 2;
pub const NFTA_CMP_DATA: u16 = 3;
pub const NFT_CMP_EQ: u32 = 0;
pub const NFT_CMP_NEQ: u32 = 1;

pub const NFTA_RANGE_SREG: u16 = 1;
pub const NFTA_RANGE_OP: u16 = 2;
pub const NFTA_RANGE_FROM_DATA: u16 = 3;
pub const NFTA_RANGE_TO_DATA: u16 = 4;
pub const NFT_RANGE_EQ: u32 = 0;

pub const NFTA_BITWISE_SREG: u16 = 1;
pub const NFTA_BITWISE_DREG: u16 = 2;
pub const NFTA_BITWISE_LEN: u16 = 3;
pub const NFTA_BITWISE_MASK: u16 = 4;
pub const NFTA_BITWISE_XOR: u16 = 5;

pub const NFTA_SOCKET_KEY: u16 = 1;
pub const NFTA_SOCKET_DREG: u16 = 2;
pub const NFT_SOCKET_MARK: u32 = 1;

pub const NFTA_IMMEDIATE_DREG: u16 = 1;
pub const NFTA_IMMEDIATE_DATA: u16 = 2;

pub const NFT_REG_VERDICT: u32 = 0;
pub const NFT_REG_1: u32 = 1;

/// `NF_DROP`.
pub const NF_DROP: u32 = 0;
/// `NF_ACCEPT`.
pub const NF_ACCEPT: u32 = 1;
/// `NF_INET_LOCAL_IN`.
pub const NF_INET_LOCAL_IN: u32 = 1;

/// libnftnl's rule userdata TLV type for a comment.
pub const NFTNL_UDATA_RULE_COMMENT: u8 = 0;
