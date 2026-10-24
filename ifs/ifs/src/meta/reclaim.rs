// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RCLLNK`: walk the whole backend (root, `/QOpenSys`, the UDFS area)
//! and make every node's record whole: a missing or undecodable record
//! is replaced by the fallback, owners and groups that no longer exist
//! become `QDFTOWN` and none, private authorities of deleted profiles
//! and lists that no longer exist go, a check-out by a deleted user is
//! ended, stray `trusted.qs400.` attributes are removed, and the
//! mirrored mode and owner are rewritten when they disagree. Link
//! counts and names that fold alike are reported, not changed.

use std::collections::HashMap;

use crate::auth::AuthEnv;
use crate::backend::{self, Backend, Kind};
use crate::errno::Result;
use crate::meta::{self, DEFAULT_OWNER, Flags, Meta, Source, XATTR};
use crate::path::fold;
use crate::vfs::{QOPENSYS_BASE, ROOT_BASE, UDFS_BASE};

/// A repair made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repair {
    /// No record: the fallback was written.
    MissingMeta,
    /// A record that would not decode: the fallback was written.
    InvalidMeta,
    /// The owner profile is gone: `QDFTOWN` owns it.
    Owner(String),
    /// The primary group is gone.
    Group(String),
    /// A private authority of a deleted profile was removed.
    Private(String),
    /// The authorization list is gone.
    Autl(String),
    /// A check-out by a deleted user was ended.
    CheckOut(String),
    /// A stray attribute was removed.
    StrayAttr(String),
    /// The mirrored mode, owner or group was rewritten.
    Mirror,
}

/// A problem found and left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The link count disagrees with the links found.
    LinkCount {
        /// What the backend says.
        nlink: u32,
        /// Links found in the walk.
        found: u32,
    },
    /// Two names in a case-insensitive directory fold alike; only the
    /// first is reachable.
    FoldCollision(String),
}

/// What `RCLLNK` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Objects (links) looked at.
    pub objects: u64,
    /// Repairs, by path.
    pub repaired: Vec<(String, Repair)>,
    /// Problems left, by path.
    pub problems: Vec<(String, Problem)>,
}

/// The namespace path a backend path shows as.
#[must_use]
pub fn shown_path(rel: &str) -> String {
    if let Some(r) = rel.strip_prefix(ROOT_BASE) {
        return if r.is_empty() {
            "/".into()
        } else {
            r.to_string()
        };
    }
    if rel.starts_with(QOPENSYS_BASE) {
        return format!("/{rel}");
    }
    if let Some(r) = rel.strip_prefix(UDFS_BASE) {
        return format!("/dev{r}");
    }
    format!("/{rel}")
}

fn fix(m: &mut Meta, env: &dyn AuthEnv, out: &mut Vec<Repair>) {
    if !env.profile_exists(&m.owner) && m.owner != DEFAULT_OWNER {
        out.push(Repair::Owner(m.owner.clone()));
        m.owner = DEFAULT_OWNER.to_string();
    }
    if let Some(g) = m.group.clone()
        && !env.profile_exists(&g)
    {
        out.push(Repair::Group(g));
        m.group = None;
        m.group_auth = sec::auth::bits::Auth::EXCLUDE;
    }
    let gone: Vec<String> = m
        .private
        .iter()
        .filter(|(p, _)| !env.profile_exists(p))
        .map(|(p, _)| p.clone())
        .collect();
    for p in gone {
        m.set_private(&p, None);
        out.push(Repair::Private(p));
    }
    if let Some(l) = m.autl.clone()
        && env.autl(&l).is_none()
    {
        out.push(Repair::Autl(l));
        m.autl = None;
        m.public_from_autl = false;
    }
    if let Some(c) = m.checkout.clone()
        && !env.profile_exists(&c.user)
    {
        out.push(Repair::CheckOut(c.user));
        m.checkout = None;
    }
}

/// Reclaim the whole backend; `check_only` reports without changing.
pub fn run<B: Backend>(b: &B, env: &dyn AuthEnv, check_only: bool) -> Result<Report> {
    let mut rep = Report::default();
    let mut links: HashMap<u64, (u32, u32, String)> = HashMap::new();
    let mut stack: Vec<(String, bool)> = Vec::new();
    for top in [ROOT_BASE, QOPENSYS_BASE, UDFS_BASE] {
        if b.stat(top).is_ok() {
            stack.push((top.to_string(), top != ROOT_BASE));
        }
    }
    while let Some((rel, case_sensitive)) = stack.pop() {
        let st = b.stat(&rel)?;
        rep.objects += 1;
        let shown = shown_path(&rel);
        let mut fixes = Vec::new();
        let (mut m, src) = meta::read(b, &rel, &st, env)?;
        match src {
            Source::Missing => fixes.push(Repair::MissingMeta),
            Source::Invalid => fixes.push(Repair::InvalidMeta),
            Source::Stored => {}
        }
        fix(&mut m, env, &mut fixes);
        let stray: Vec<String> = b
            .xattr_list(&rel)?
            .into_iter()
            .filter(|n| n.starts_with("trusted.qs400.") && n != XATTR)
            .collect();
        let mirror_bad = st.kind != Kind::Symlink
            && (st.mode & 0o7777 != crate::auth::data::mode_of(&m)
                || st.uid != env.uid_of(&m.owner).unwrap_or(0)
                || st.gid != m.group.as_deref().and_then(|g| env.gid_of(g)).unwrap_or(0));
        if !check_only {
            if !fixes.is_empty() {
                meta::write(b, &rel, st.kind, &m, env)?;
            } else if mirror_bad {
                meta::mirror(b, &rel, st.kind, &m, env)?;
            }
            for n in &stray {
                b.xattr_remove(&rel, n)?;
            }
        }
        if fixes.is_empty() && mirror_bad {
            fixes.push(Repair::Mirror);
        }
        fixes.extend(stray.into_iter().map(Repair::StrayAttr));
        rep.repaired
            .extend(fixes.into_iter().map(|r| (shown.clone(), r)));
        if st.kind == Kind::Dir {
            let ents = b.readdir(&rel)?;
            let cs = case_sensitive || m.flags.has(Flags::CASE_SENSITIVE);
            if !cs {
                let mut seen: HashMap<String, &str> = HashMap::new();
                for e in &ents {
                    if let Some(first) = seen.insert(fold::fold(&e.name), &e.name) {
                        rep.problems.push((
                            shown_path(&backend::join(&rel, &e.name)),
                            Problem::FoldCollision(first.to_string()),
                        ));
                    }
                }
            }
            // The UDFS area's ASP directories hold UDFS roots.
            let child_cs = if rel.starts_with(UDFS_BASE) && rel.matches('/').count() < 2 {
                false
            } else {
                cs
            };
            for e in ents.into_iter().rev() {
                stack.push((backend::join(&rel, &e.name), child_cs));
            }
        } else {
            let ent = links.entry(st.ino).or_insert((st.nlink, 0, shown));
            ent.1 += 1;
        }
    }
    let mut counts: Vec<_> = links
        .into_values()
        .filter(|(n, f, _)| n != f)
        .map(|(nlink, found, p)| (p, Problem::LinkCount { nlink, found }))
        .collect();
    counts.sort_by(|a, b| a.0.cmp(&b.0));
    rep.problems.extend(counts);
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shown_paths() {
        assert_eq!(shown_path("root"), "/");
        assert_eq!(shown_path("root/home/a"), "/home/a");
        assert_eq!(shown_path("QOpenSys/usr"), "/QOpenSys/usr");
        assert_eq!(shown_path("UDFS/QASP01/X.udfs/f"), "/dev/QASP01/X.udfs/f");
    }
}
