// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! nftables over `NETLINK_NETFILTER`: the backstop for TCP/IP port
//! restrictions (`ADDTCPPORT`).
//!
//! QS/400 owns one `inet` table, [`TABLE`], with one base chain,
//! [`CHAIN`], on the input hook at filter priority, policy accept. Each
//! restriction is one rule:
//!
//! ```text
//! meta l4proto tcp th dport 8000-8100 socket mark & 0xffff0000 != 0x51530000 counter drop
//! ```
//!
//! The sockets API sets [`SOCKET_MARK`] (`SO_MARK`) on every socket a
//! QS/400 job binds after it passes the restriction check, so packets
//! for those sockets pass; anything else listening on the port (a socket
//! not made through the API) never sees them. See docs/TCPIP.md.

pub mod consts;
mod expr;

#[allow(clippy::wildcard_imports)] // the numbers module
use consts::*;
pub use expr::port_drop_exprs;

use crate::codec::{Attrs, Builder, NLM_F_ACK, NLM_F_APPEND, NLM_F_CREATE, NLM_F_DUMP, NLM_F_REQUEST, NLMSG_ERROR, messages};
use crate::{Bus, Error, Result, Socket};

/// The QS/400 table.
pub const TABLE: &str = "qs400";
/// Its input chain.
pub const CHAIN: &str = "input";
/// The mark the sockets API sets on sockets bound through it. The low
/// 16 bits are free for other uses.
pub const SOCKET_MARK: u32 = 0x5153_0000;
/// The bits of a socket mark that [`SOCKET_MARK`] occupies.
pub const SOCKET_MARK_MASK: u32 = 0xffff_0000;

/// Transport protocol of a restriction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum L4 {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

impl L4 {
    /// `IPPROTO_*`.
    pub fn number(self) -> u8 {
        match self {
            L4::Tcp => 6,
            L4::Udp => 17,
        }
    }

    fn word(self) -> &'static str {
        match self {
            L4::Tcp => "tcp",
            L4::Udp => "udp",
        }
    }
}

/// Drop `proto` to ports `first..=last` unless the receiving socket
/// carries [`SOCKET_MARK`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PortRule {
    /// Protocol.
    pub proto: L4,
    /// First port.
    pub first: u16,
    /// Last port (equal to `first` for one port).
    pub last: u16,
}

impl PortRule {
    /// A rule for one port or a range.
    pub fn new(proto: L4, first: u16, last: u16) -> Result<Self> {
        if first == 0 || last < first {
            return Err(Error::Invalid(format!("port range {first}-{last}")));
        }
        Ok(Self { proto, first, last })
    }

    /// The comment stored with the rule, which identifies it.
    pub fn tag(&self) -> String {
        format!("qs400 {} {}-{}", self.proto.word(), self.first, self.last)
    }
}

/// A rule in the QS/400 chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleInfo {
    /// Kernel handle.
    pub handle: u64,
    /// Comment, when the rule has one.
    pub comment: Option<String>,
}

fn nfgenmsg(family: u8, res_id: u16) -> [u8; 4] {
    let r = res_id.to_be_bytes();
    [family, NFNETLINK_V0, r[0], r[1]]
}

fn msg(kind: u16, flags: u16) -> Builder {
    let mut b = Builder::new((NFNL_SUBSYS_NFTABLES << 8) | kind, NLM_F_REQUEST | flags);
    b.bytes(&nfgenmsg(NFPROTO_INET, 0));
    b
}

/// Create the table if missing.
pub fn new_table() -> Builder {
    let mut b = msg(NFT_MSG_NEWTABLE, NLM_F_CREATE | NLM_F_ACK);
    b.str(NFTA_TABLE_NAME, TABLE).be32(NFTA_TABLE_FLAGS, 0);
    b
}

/// Delete the table and everything in it.
pub fn del_table() -> Builder {
    let mut b = msg(NFT_MSG_DELTABLE, NLM_F_ACK);
    b.str(NFTA_TABLE_NAME, TABLE);
    b
}

/// Create the input chain if missing.
pub fn new_chain() -> Builder {
    let mut b = msg(NFT_MSG_NEWCHAIN, NLM_F_CREATE | NLM_F_ACK);
    b.str(NFTA_CHAIN_TABLE, TABLE).str(NFTA_CHAIN_NAME, CHAIN);
    b.nest(NFTA_CHAIN_HOOK, |b| {
        b.be32(NFTA_HOOK_HOOKNUM, NF_INET_LOCAL_IN)
            .be32(NFTA_HOOK_PRIORITY, 0);
    });
    b.be32(NFTA_CHAIN_POLICY, NF_ACCEPT)
        .str(NFTA_CHAIN_TYPE, "filter");
    b
}

/// Append a port restriction rule.
pub fn new_rule(rule: &PortRule) -> Builder {
    let mut b = msg(NFT_MSG_NEWRULE, NLM_F_CREATE | NLM_F_APPEND | NLM_F_ACK);
    b.str(NFTA_RULE_TABLE, TABLE).str(NFTA_RULE_CHAIN, CHAIN);
    b.nest(NFTA_RULE_EXPRESSIONS, |b| port_drop_exprs(b, rule));
    b.attr(NFTA_RULE_USERDATA, &comment_udata(&rule.tag()));
    b
}

/// Delete a rule by handle.
pub fn del_rule(handle: u64) -> Builder {
    let mut b = msg(NFT_MSG_DELRULE, NLM_F_ACK);
    b.str(NFTA_RULE_TABLE, TABLE)
        .str(NFTA_RULE_CHAIN, CHAIN)
        .be64(NFTA_RULE_HANDLE, handle);
    b
}

/// Dump the rules of the QS/400 table.
pub fn list_rules() -> Builder {
    let mut b = msg(NFT_MSG_GETRULE, NLM_F_DUMP);
    b.str(NFTA_RULE_TABLE, TABLE);
    b
}

/// The libnftnl comment TLV, so `nft list ruleset` shows the tag.
pub fn comment_udata(text: &str) -> Vec<u8> {
    let mut v = vec![NFTNL_UDATA_RULE_COMMENT, 0];
    v.extend_from_slice(text.as_bytes());
    v.push(0);
    v[1] = u8::try_from(v.len() - 2).unwrap_or(u8::MAX);
    v
}

fn parse_comment(udata: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + 2 <= udata.len() {
        let (ty, len) = (udata[i], udata[i + 1] as usize);
        let val = udata.get(i + 2..i + 2 + len)?;
        if ty == NFTNL_UDATA_RULE_COMMENT {
            let end = val.iter().position(|b| *b == 0).unwrap_or(val.len());
            return Some(String::from_utf8_lossy(&val[..end]).into_owned());
        }
        i += 2 + len;
    }
    None
}

/// Parse an `NFT_MSG_NEWRULE` payload.
pub fn parse_rule(payload: &[u8]) -> Option<RuleInfo> {
    let attrs = Attrs::new(payload.get(4..)?);
    Some(RuleInfo {
        handle: attrs.get(NFTA_RULE_HANDLE)?.be64()?,
        comment: attrs
            .get(NFTA_RULE_USERDATA)
            .and_then(|a| parse_comment(a.data)),
    })
}

/// Wrap `cmds` in a batch: begin, the commands, end, with sequence
/// numbers from `seq`. Returns the bytes and the commands' numbers.
pub fn batch(cmds: &[Builder], seq: u32) -> (Vec<u8>, Vec<u32>) {
    let mut begin = Builder::new(NFNL_MSG_BATCH_BEGIN, NLM_F_REQUEST);
    begin.bytes(&nfgenmsg(0, NFNL_SUBSYS_NFTABLES));
    let mut end = Builder::new(NFNL_MSG_BATCH_END, NLM_F_REQUEST);
    end.bytes(&nfgenmsg(0, NFNL_SUBSYS_NFTABLES));
    let mut out = begin.finish(seq);
    let mut seqs = Vec::new();
    let mut s = seq;
    for c in cmds {
        s = s.wrapping_add(1);
        out.extend_from_slice(&c.finish(s));
        seqs.push(s);
    }
    out.extend_from_slice(&end.finish(s.wrapping_add(1)));
    (out, seqs)
}

/// An nftables client for the QS/400 table.
pub struct Nft {
    sock: Socket,
}

impl Nft {
    /// Open a request socket.
    pub fn open() -> Result<Self> {
        Ok(Self {
            sock: Socket::open(Bus::Netfilter)?,
        })
    }

    /// Run commands as one transaction; all apply or none.
    pub fn transact(&mut self, cmds: &[Builder]) -> Result<()> {
        let first = self.sock.next_seq();
        let (bytes, seqs) = batch(cmds, first);
        for _ in 0..=seqs.len() {
            self.sock.next_seq();
        }
        self.sock.send_bytes(&bytes)?;
        let mut pending = seqs.clone();
        let mut failure = None;
        while !pending.is_empty() {
            for m in messages(self.sock.recv()?) {
                let m = m?;
                if m.kind != NLMSG_ERROR || !pending.contains(&m.seq) {
                    continue;
                }
                pending.retain(|s| *s != m.seq);
                match m.error() {
                    Some(0) => {}
                    Some(e) => {
                        failure.get_or_insert(Error::Kernel(e));
                    }
                    None => return Err(Error::Malformed("error message")),
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Create the table and chain if missing.
    pub fn ensure(&mut self) -> Result<()> {
        self.transact(&[new_table(), new_chain()])
    }

    /// Delete the table, every rule with it. Missing is not an error.
    pub fn remove_all(&mut self) -> Result<()> {
        match self.transact(&[del_table()]) {
            Err(e) if e.is_not_found() => Ok(()),
            r => r,
        }
    }

    /// The rules in the chain. Empty when the table does not exist.
    pub fn rules(&mut self) -> Result<Vec<RuleInfo>> {
        match self.sock.dump(&list_rules(), |m| Ok(parse_rule(m.payload))) {
            Err(e) if e.is_not_found() => Ok(Vec::new()),
            r => r,
        }
    }

    /// Add a restriction (creating the table if needed); returns its handle.
    pub fn add_port_drop(&mut self, rule: &PortRule) -> Result<u64> {
        self.transact(&[new_table(), new_chain(), new_rule(rule)])?;
        let tag = rule.tag();
        self.rules()?
            .into_iter()
            .rev()
            .find(|r| r.comment.as_deref() == Some(tag.as_str()))
            .map(|r| r.handle)
            .ok_or(Error::Malformed("added rule not listed"))
    }

    /// Remove every rule for a restriction. Returns how many went.
    pub fn remove_port_drop(&mut self, rule: &PortRule) -> Result<usize> {
        let tag = rule.tag();
        let dels: Vec<Builder> = self
            .rules()?
            .into_iter()
            .filter(|r| r.comment.as_deref() == Some(tag.as_str()))
            .map(|r| del_rule(r.handle))
            .collect();
        if !dels.is_empty() {
            self.transact(&dels)?;
        }
        Ok(dels.len())
    }

    /// Delete one rule by handle.
    pub fn delete_rule(&mut self, handle: u64) -> Result<()> {
        self.transact(&[del_rule(handle)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{NLMSG_HDRLEN, ne_u16};

    #[test]
    fn rule_carries_the_expressions_in_order() {
        let r = PortRule::new(L4::Tcp, 8000, 8100).unwrap();
        let m = new_rule(&r).finish(9);
        assert_eq!(ne_u16(&m, 4), Some((NFNL_SUBSYS_NFTABLES << 8) | NFT_MSG_NEWRULE));
        assert_eq!(m[NLMSG_HDRLEN], NFPROTO_INET);
        let attrs = Attrs::new(&m[NLMSG_HDRLEN + 4..]);
        assert_eq!(attrs.get(NFTA_RULE_TABLE).unwrap().str(), TABLE);
        assert_eq!(attrs.get(NFTA_RULE_CHAIN).unwrap().str(), CHAIN);
        let names: Vec<String> = attrs
            .get(NFTA_RULE_EXPRESSIONS)
            .unwrap()
            .attrs()
            .map(|e| e.attrs().get(NFTA_EXPR_NAME).unwrap().str())
            .collect();
        assert_eq!(
            names,
            ["meta", "cmp", "payload", "range", "socket", "bitwise", "cmp", "counter", "immediate"]
        );
        let udata = attrs.get(NFTA_RULE_USERDATA).unwrap();
        assert_eq!(parse_comment(udata.data).as_deref(), Some("qs400 tcp 8000-8100"));

        // One port compares instead of a range.
        let one = new_rule(&PortRule::new(L4::Udp, 53, 53).unwrap()).finish(1);
        let a = Attrs::new(&one[NLMSG_HDRLEN + 4..]);
        let names: Vec<String> = a
            .get(NFTA_RULE_EXPRESSIONS)
            .unwrap()
            .attrs()
            .map(|e| e.attrs().get(NFTA_EXPR_NAME).unwrap().str())
            .collect();
        assert_eq!(names[3], "cmp");
    }

    #[test]
    fn batch_wraps_commands_with_consecutive_numbers() {
        let (bytes, seqs) = batch(&[new_table(), new_chain()], 40);
        assert_eq!(seqs, [41, 42]);
        let ms: Vec<_> = messages(&bytes).map(Result::unwrap).collect();
        assert_eq!(ms.len(), 4);
        assert_eq!(ms[0].kind, NFNL_MSG_BATCH_BEGIN);
        assert_eq!(ms[0].seq, 40);
        assert_eq!(&ms[0].payload[2..4], &NFNL_SUBSYS_NFTABLES.to_be_bytes());
        assert_eq!(ms[3].kind, NFNL_MSG_BATCH_END);
        assert_eq!(ms[3].seq, 43);
        assert!(ms[1].flags & NLM_F_ACK != 0);
    }

    #[test]
    fn port_rules_validate() {
        assert!(PortRule::new(L4::Tcp, 0, 10).is_err());
        assert!(PortRule::new(L4::Tcp, 20, 10).is_err());
        assert_eq!(comment_udata("ab"), [0, 3, b'a', b'b', 0]);
    }
}
