// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `config` backend's interface (docs/DIRSRV.md §8, §10):
//! `cn=configuration`, a view of the instance's `ibmslapd.conf`. The
//! service implements [`ConfigSource`] over the file (`crate::config`
//! reads and writes it); [`ConfigBackend`] serves it, to administrators
//! only (50 for others).

use ldap::ResultCode;
use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{Attribute, Modification, Scope};
use store::txn::Txn;

use super::{Backend, DirEntry, Resolution, Scan};
use crate::ops::error::{OpError, OpResult};
use crate::ops::session::Session;
use crate::schema::Schema;

/// The suffix.
pub const DN: &str = "cn=configuration";

/// The configuration file's entries, as the service holds them.
pub trait ConfigSource: Send {
    /// Every entry, parents before children, `cn=configuration` first.
    fn entries(&self) -> Vec<(String, Vec<Attribute>)>;

    /// Apply a modify of `dn`: written to the file at once, in force as
    /// §10 says.
    ///
    /// # Errors
    /// The result code and Quartz's text.
    fn modify(&mut self, dn: &str, mods: &[Modification]) -> Result<(), (ResultCode, String)>;
}

/// A [`ConfigSource`] as a backend. Entry IDs are positions in
/// [`ConfigSource::entries`], plus one.
pub struct ConfigBackend<C: ConfigSource> {
    /// The source.
    pub source: std::sync::Mutex<C>,
}

impl<C: ConfigSource> ConfigBackend<C> {
    /// The backend over `source`.
    pub fn new(source: C) -> Self {
        Self {
            source: std::sync::Mutex::new(source),
        }
    }

    fn all(&self) -> Vec<(String, Vec<Attribute>)> {
        self.source.lock().map(|s| s.entries()).unwrap_or_default()
    }

    fn parsed(&self) -> Vec<(Dn, String, Vec<Attribute>)> {
        self.all()
            .into_iter()
            .filter_map(|(dn, a)| Dn::parse(&dn).ok().map(|d| (d, dn, a)))
            .collect()
    }

    /// Apply a modify to entry `id`.
    ///
    /// # Errors
    /// The source's.
    pub fn modify(&self, id: u64, mods: &[Modification]) -> OpResult<()> {
        // The file's values are text: nothing to increment.
        if mods.iter().any(|m| m.op == ldap::msg::ModOp::Increment) {
            return Err(OpError::new(
                ResultCode::UnwillingToPerform,
                "the configuration takes no increment",
            ));
        }
        let all = self.all();
        let i = usize::try_from(id.saturating_sub(1)).unwrap_or(usize::MAX);
        let (dn, _) = all
            .get(i)
            .ok_or_else(|| OpError::new(ResultCode::NoSuchObject, "no such entry"))?;
        let mut s = self
            .source
            .lock()
            .map_err(|_| OpError::new(ResultCode::OperationsError, "configuration unavailable"))?;
        s.modify(dn, mods).map_err(|(c, m)| OpError::new(c, m))
    }
}

impl<C: ConfigSource> Backend for ConfigBackend<C> {
    fn name(&self) -> &'static str {
        "config"
    }

    fn suffixes(&self) -> Vec<Dn> {
        vec![Dn::parse(DN).expect("a DN")]
    }

    fn admin_only(&self) -> bool {
        true
    }

    fn resolve(
        &self,
        _txn: &mut Txn<'_>,
        schema: &Schema,
        _session: &Session,
        dn: &Dn,
    ) -> OpResult<Resolution> {
        let all = self.parsed();
        let pos = |d: &Dn| all.iter().position(|(x, _, _)| schema.dn_eq(x, d));
        let entry = pos(dn).map(|i| i as u64 + 1);
        let mut up = dn.parent();
        let mut matched = None;
        while let Some(p) = up {
            if let Some(i) = pos(&p) {
                matched = Some(i as u64 + 1);
                break;
            }
            up = p.parent();
        }
        Ok(Resolution { entry, matched })
    }

    fn entry(
        &self,
        _txn: &mut Txn<'_>,
        _schema: &Schema,
        _session: &Session,
        id: u64,
    ) -> OpResult<Option<DirEntry>> {
        let i = usize::try_from(id.saturating_sub(1)).unwrap_or(usize::MAX);
        Ok(self
            .all()
            .into_iter()
            .nth(i)
            .map(|(dn, attrs)| DirEntry { id, dn, attrs }))
    }

    fn parent(&self, _txn: &mut Txn<'_>, id: u64) -> OpResult<Option<u64>> {
        let all = self.parsed();
        let i = usize::try_from(id.saturating_sub(1)).unwrap_or(usize::MAX);
        let Some((dn, _, _)) = all.get(i) else {
            return Ok(None);
        };
        let Some(p) = dn.parent() else {
            return Ok(None);
        };
        Ok(all
            .iter()
            .position(|(x, _, _)| x.raw_eq(&p))
            .map(|k| k as u64 + 1))
    }

    fn children(&self, txn: &mut Txn<'_>, id: u64, cap: usize) -> OpResult<u64> {
        let n = self.parsed().len() as u64;
        let mut c = 0;
        for k in 1..=n {
            if usize::try_from(c).unwrap_or(usize::MAX) >= cap {
                break;
            }
            if self.parent(txn, k)? == Some(id) {
                c += 1;
            }
        }
        Ok(c)
    }

    fn apply(&self, _txn: &mut Txn<'_>, id: u64, mods: &[Modification]) -> Option<OpResult<()>> {
        Some(self.modify(id, mods))
    }

    fn scan(
        &self,
        txn: &mut Txn<'_>,
        _schema: &Schema,
        _session: &Session,
        base: u64,
        scope: Scope,
        _filter: &Filter,
        state: &mut Scan,
        _max: usize,
    ) -> OpResult<Vec<u64>> {
        if matches!(state, Scan::Done) {
            return Ok(Vec::new());
        }
        *state = Scan::Done;
        let n = self.parsed().len() as u64;
        let mut out = Vec::new();
        for k in 1..=n {
            let take = match scope {
                Scope::Base => k == base,
                Scope::One => self.parent(txn, k)? == Some(base),
                Scope::Sub => {
                    let mut cur = Some(k);
                    let mut hit = false;
                    while let Some(c) = cur {
                        if c == base {
                            hit = true;
                            break;
                        }
                        cur = self.parent(txn, c)?;
                    }
                    hit
                }
            };
            if take {
                out.push(k);
            }
        }
        Ok(out)
    }
}
