// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `TAPnn` and `TAPMLBnn` resource names from a scan of the tape drives
//! (`/sys/class/scsi_tape`) and the changers' `sg` nodes.
//!
//! Resources are sorted by SCSI address (host, channel, target, LUN)
//! and then node. A resource whose serial number already had a name
//! keeps it; the rest take the lowest free numbers in sorted order.

use std::collections::BTreeSet;

/// What kind of resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    /// A tape drive: `TAPnn`.
    Tape,
    /// A media changer: `TAPMLBnn`.
    Library,
}

impl ResourceKind {
    /// The resource name prefix.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Tape => "TAP",
            Self::Library => "TAPMLB",
        }
    }
}

/// One device the scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scanned {
    /// Drive or changer.
    pub kind: ResourceKind,
    /// The node: `st0` for a drive, `sg3` for a changer.
    pub node: String,
    /// SCSI host, channel, target and LUN.
    pub hctl: [u32; 4],
    /// The unit serial number, when the device reports one.
    pub serial: Option<String>,
}

/// A resource with its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// `TAPnn` or `TAPMLBnn`.
    pub name: String,
    /// The device.
    pub device: Scanned,
}

/// Parse `H:C:T:L` (a SCSI device's sysfs name).
#[must_use]
pub fn parse_hctl(s: &str) -> Option<[u32; 4]> {
    let mut out = [0u32; 4];
    let mut it = s.split(':');
    for o in &mut out {
        *o = it.next()?.parse().ok()?;
    }
    it.next().is_none().then_some(out)
}

/// The drive nodes among `/sys/class/scsi_tape` entries: `stN` only
/// (not `nstN` or the mode variants `stNa`, `stNl`, `stNm`).
#[must_use]
pub fn tape_nodes<S: AsRef<str>>(entries: &[S]) -> Vec<String> {
    let mut v: Vec<String> = entries
        .iter()
        .map(AsRef::as_ref)
        .filter(|e| {
            e.strip_prefix("st")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_string)
        .collect();
    v.sort_by_key(|n| node_number(n));
    v
}

fn node_number(node: &str) -> u32 {
    node.trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .unwrap_or(u32::MAX)
}

/// Name every scanned device. `known` holds (serial, name) pairs from
/// earlier scans.
#[must_use]
pub fn assign(scanned: &[Scanned], known: &[(String, String)]) -> Vec<Resource> {
    let mut sorted: Vec<&Scanned> = scanned.iter().collect();
    sorted.sort_by(|a, b| {
        (a.kind, a.hctl, node_number(&a.node), &a.node).cmp(&(
            b.kind,
            b.hctl,
            node_number(&b.node),
            &b.node,
        ))
    });
    let number = |kind: ResourceKind, name: &str| -> Option<u32> {
        let rest = name.strip_prefix(kind.prefix())?;
        if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest.parse().ok().filter(|&n| n > 0)
    };
    let mut used: BTreeSet<(ResourceKind, u32)> = BTreeSet::new();
    let mut names: Vec<Option<String>> = vec![None; sorted.len()];
    for (i, d) in sorted.iter().enumerate() {
        let prior = d.serial.as_ref().and_then(|s| {
            known
                .iter()
                .find(|(ks, kn)| ks == s && number(d.kind, kn).is_some())
                .map(|(_, kn)| kn.clone())
        });
        if let Some(name) = prior {
            let n = number(d.kind, &name).expect("checked");
            if used.insert((d.kind, n)) {
                names[i] = Some(name);
            }
        }
    }
    let mut out = Vec::with_capacity(sorted.len());
    for (i, d) in sorted.iter().enumerate() {
        let name = names[i].take().unwrap_or_else(|| {
            let mut n = 1;
            while used.contains(&(d.kind, n)) {
                n += 1;
            }
            used.insert((d.kind, n));
            format!("{}{n:02}", d.kind.prefix())
        });
        out.push(Resource {
            name,
            device: (*d).clone(),
        });
    }
    out
}
