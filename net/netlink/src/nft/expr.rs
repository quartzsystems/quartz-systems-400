// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The expressions of a port restriction rule. Register data (the
//! protocol byte, ports, the mark) is in packet or host order as the
//! kernel loads it; expression parameters are big-endian.

#[allow(clippy::wildcard_imports)] // the numbers module
use super::consts::*;
use super::{PortRule, SOCKET_MARK, SOCKET_MARK_MASK};
use crate::codec::Builder;

fn expr(b: &mut Builder, name: &str, data: impl FnOnce(&mut Builder)) {
    b.nest(NFTA_LIST_ELEM, |b| {
        b.str(NFTA_EXPR_NAME, name);
        b.nest(NFTA_EXPR_DATA, data);
    });
}

fn value(b: &mut Builder, kind: u16, bytes: &[u8]) {
    b.nest(kind, |b| {
        b.attr(NFTA_DATA_VALUE, bytes);
    });
}

fn cmp(b: &mut Builder, op: u32, bytes: &[u8]) {
    expr(b, "cmp", |b| {
        b.be32(NFTA_CMP_SREG, NFT_REG_1).be32(NFTA_CMP_OP, op);
        value(b, NFTA_CMP_DATA, bytes);
    });
}

/// Append the rule's expressions inside `NFTA_RULE_EXPRESSIONS`.
pub fn port_drop_exprs(b: &mut Builder, rule: &PortRule) {
    // meta l4proto <proto>
    expr(b, "meta", |b| {
        b.be32(NFTA_META_KEY, NFT_META_L4PROTO)
            .be32(NFTA_META_DREG, NFT_REG_1);
    });
    cmp(b, NFT_CMP_EQ, &[rule.proto.number()]);
    // th dport: two bytes at offset 2 of the transport header.
    expr(b, "payload", |b| {
        b.be32(NFTA_PAYLOAD_DREG, NFT_REG_1)
            .be32(NFTA_PAYLOAD_BASE, NFT_PAYLOAD_TRANSPORT_HEADER)
            .be32(NFTA_PAYLOAD_OFFSET, 2)
            .be32(NFTA_PAYLOAD_LEN, 2);
    });
    if rule.first == rule.last {
        cmp(b, NFT_CMP_EQ, &rule.first.to_be_bytes());
    } else {
        expr(b, "range", |b| {
            b.be32(NFTA_RANGE_SREG, NFT_REG_1)
                .be32(NFTA_RANGE_OP, NFT_RANGE_EQ);
            value(b, NFTA_RANGE_FROM_DATA, &rule.first.to_be_bytes());
            value(b, NFTA_RANGE_TO_DATA, &rule.last.to_be_bytes());
        });
    }
    // socket mark & mask != SOCKET_MARK. No socket: the rule stops here
    // and the kernel refuses the packet itself.
    expr(b, "socket", |b| {
        b.be32(NFTA_SOCKET_KEY, NFT_SOCKET_MARK)
            .be32(NFTA_SOCKET_DREG, NFT_REG_1);
    });
    expr(b, "bitwise", |b| {
        b.be32(NFTA_BITWISE_SREG, NFT_REG_1)
            .be32(NFTA_BITWISE_DREG, NFT_REG_1)
            .be32(NFTA_BITWISE_LEN, 4);
        value(b, NFTA_BITWISE_MASK, &SOCKET_MARK_MASK.to_ne_bytes());
        value(b, NFTA_BITWISE_XOR, &0u32.to_ne_bytes());
    });
    cmp(b, NFT_CMP_NEQ, &SOCKET_MARK.to_ne_bytes());
    expr(b, "counter", |_| {});
    expr(b, "immediate", |b| {
        b.be32(NFTA_IMMEDIATE_DREG, NFT_REG_VERDICT);
        b.nest(NFTA_IMMEDIATE_DATA, |b| {
            b.nest(NFTA_DATA_VERDICT, |b| {
                b.be32(NFTA_VERDICT_CODE, NF_DROP);
            });
        });
    });
}
