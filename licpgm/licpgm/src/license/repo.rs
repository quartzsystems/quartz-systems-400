// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The license repository: installed keys (ADDLICKEY, RMVLICKEY,
//! DSPLICKEY) and per-feature license information (WRKLICINF,
//! CHGLICINF) with its current users.

use release::Release;

use crate::id::{LicTerm, LoadId, ProductId};
use crate::license::LicError;
use crate::license::key::{self, KeyFields, LicenseKey};
use crate::model::definition::{Compliance, ProductDefinition, UsageType};
use crate::model::{Date, QualName};
use crate::system::SystemInfo;

/// A key ADDLICKEY installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledKey {
    /// Its fields.
    pub fields: KeyFields,
    /// The key.
    pub key: LicenseKey,
    /// When it was added.
    pub added: Date,
}

/// One holder of license uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseUser {
    /// The user name, `*JOB`'s job name or `*PROCESSOR`.
    pub name: String,
    /// The requester's handle; never shown.
    pub handle: [u8; 8],
    /// Uses held.
    pub uses: u32,
}

/// The license information of one product, term and feature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseInfo {
    /// The product.
    pub product: ProductId,
    /// The term.
    pub term: LicTerm,
    /// The feature.
    pub feature: LoadId,
    /// How use is counted.
    pub usage_type: UsageType,
    /// What happens past the limit.
    pub compliance: Compliance,
    /// The usage limit for non-keyed compliance; `None` is `*NOMAX`.
    pub usage_limit: Option<u32>,
    /// Threshold; `None` is none.
    pub threshold: Option<u32>,
    /// Grace period, days.
    pub grace_days: u16,
    /// When the current grace period began.
    pub grace_start: Option<Date>,
    /// Where notices go.
    pub msgq: QualName,
    /// Whether requests past the limit are logged.
    pub log: bool,
    /// The highest use count seen.
    pub peak: u32,
    /// Current holders.
    pub users: Vec<LicenseUser>,
}

impl LicenseInfo {
    /// Uses held now.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.users.iter().map(|u| u.uses).sum()
    }
}

/// Keys and license information.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LicenseRepo {
    /// Installed keys.
    pub keys: Vec<InstalledKey>,
    /// License information, one per product, term and feature.
    pub info: Vec<LicenseInfo>,
}

/// An ADDLICKEY request.
#[derive(Clone, Debug)]
pub struct AddLicKey {
    /// The fields; `serial` may be `*LOCAL`.
    pub fields: KeyFields,
    /// The key.
    pub key: LicenseKey,
}

/// CHGLICINF's changes; `None` leaves a value as it is.
#[derive(Clone, Debug, Default)]
pub struct ChgLicInf {
    /// `USGLMT`: `Some(None)` is `*NOMAX`.
    pub usage_limit: Option<Option<u32>>,
    /// `THRESHOLD`: `Some(None)` is `*USGLMT` (none).
    pub threshold: Option<Option<u32>>,
    /// `MSGQ`.
    pub msgq: Option<QualName>,
    /// `LOG`.
    pub log: Option<bool>,
}

/// One DSPLICKEY entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRow {
    /// Product.
    pub product: String,
    /// Term.
    pub term: String,
    /// Feature.
    pub feature: String,
    /// Serial number.
    pub serial: String,
    /// Processor group.
    pub processor_group: String,
    /// Usage limit or `*NOMAX`.
    pub usage_limit: String,
    /// Expiration date or `*NONE`.
    pub expiration: String,
    /// Vendor data.
    pub vendor_data: String,
    /// The key, six lines of three groups.
    pub key: Vec<String>,
}

/// One WRKLICINF entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoRow {
    /// Product.
    pub product: String,
    /// Term.
    pub term: String,
    /// Feature.
    pub feature: String,
    /// `*KEYED`, `*WARNING`, `*OPRACTION`.
    pub compliance: &'static str,
    /// `*CONCURRENT`, ...
    pub usage_type: &'static str,
    /// The effective usage limit or `*NOMAX`.
    pub usage_limit: String,
    /// Uses held.
    pub usage_count: u32,
    /// The highest count seen.
    pub peak: u32,
    /// Threshold or blank.
    pub threshold: String,
    /// Grace period days.
    pub grace_days: u16,
    /// When the running grace period ends, if one runs.
    pub grace_end: Option<Date>,
    /// The key's expiration date, if keyed.
    pub expiration: Option<Date>,
}

fn nomax(n: Option<u32>) -> String {
    n.map_or_else(|| "*NOMAX".to_string(), |n| n.to_string())
}

impl LicenseRepo {
    /// Add license information for every license term of a definition
    /// that has none yet (on install).
    pub fn sync_definition(&mut self, d: &ProductDefinition) {
        for l in &d.licenses {
            let have = self
                .info
                .iter()
                .any(|i| i.product == d.product && i.term == l.term && i.feature == l.feature);
            if !have {
                self.info.push(LicenseInfo {
                    product: d.product.clone(),
                    term: l.term.clone(),
                    feature: l.feature.clone(),
                    usage_type: l.usage_type,
                    compliance: l.compliance,
                    usage_limit: l.usage_limit,
                    threshold: l.threshold,
                    grace_days: l.grace_days,
                    grace_start: None,
                    msgq: QualName::new("QSYSOPR", "*LIBL"),
                    log: false,
                    peak: 0,
                    users: Vec::new(),
                });
            }
        }
    }

    /// Drop the license information of a product (DLTLICPGM); keys stay.
    pub fn forget_product(&mut self, p: &ProductId) {
        self.info.retain(|i| &i.product != p);
    }

    /// The information for a product, a release in its term, and a
    /// feature.
    #[must_use]
    pub fn info_for(&self, p: &ProductId, rel: Release, f: &LoadId) -> Option<&LicenseInfo> {
        self.info
            .iter()
            .find(|i| &i.product == p && &i.feature == f && i.term.covers(rel))
    }

    fn info_index(&self, p: &ProductId, term: &LicTerm, f: &LoadId) -> Result<usize, LicError> {
        self.info
            .iter()
            .position(|i| &i.product == p && &i.term == term && &i.feature == f)
            .ok_or(LicError::NoInfo)
    }

    /// The keys valid on this system for an information entry today.
    #[must_use]
    pub fn valid_keys(
        &self,
        info: &LicenseInfo,
        sys: &SystemInfo,
        today: Date,
    ) -> Vec<&InstalledKey> {
        self.keys
            .iter()
            .filter(|k| {
                let f = &k.fields;
                f.product == info.product
                    && f.term == info.term
                    && f.feature == info.feature
                    && f.serial == sys.serial
                    && (f.processor_group == "*ANY" || f.processor_group == sys.processor_group)
                    && !f.expired(today)
            })
            .collect()
    }

    /// The usage limit in force: the keyed limit (the highest valid
    /// key's, 0 with none) or the information's own.
    #[must_use]
    pub fn effective_limit(
        &self,
        info: &LicenseInfo,
        sys: &SystemInfo,
        today: Date,
    ) -> Option<u32> {
        if info.compliance != Compliance::Keyed {
            return info.usage_limit;
        }
        let keys = self.valid_keys(info, sys, today);
        if keys.iter().any(|k| k.fields.usage_limit.is_none()) {
            return None;
        }
        Some(
            keys.iter()
                .filter_map(|k| k.fields.usage_limit)
                .max()
                .unwrap_or(0),
        )
    }

    /// ADDLICKEY: check and install a key. The public key comes from
    /// the product's definition, else `quartz_key`.
    pub fn add_key(
        &mut self,
        defs: &[ProductDefinition],
        quartz_key: Option<&[u8; 32]>,
        sys: &SystemInfo,
        req: AddLicKey,
        today: Date,
    ) -> Result<(), LicError> {
        let mut fields = req.fields;
        if fields.serial == "*LOCAL" {
            fields.serial.clone_from(&sys.serial);
        }
        let def = defs
            .iter()
            .find(|d| {
                d.product == fields.product
                    && d.licenses
                        .iter()
                        .any(|l| l.term == fields.term && l.feature == fields.feature)
            })
            .ok_or(LicError::NoInfo)?;
        if fields.serial != sys.serial {
            return Err(LicError::SerialMismatch);
        }
        if fields.processor_group != "*ANY" && fields.processor_group != sys.processor_group {
            return Err(LicError::ProcessorGroup);
        }
        if fields.expired(today) {
            return Err(LicError::Expired);
        }
        let public = def
            .license_key
            .as_ref()
            .or(quartz_key)
            .ok_or(LicError::NoPublicKey)?;
        key::verify(public, &fields, &req.key).map_err(LicError::Key)?;
        self.keys.retain(|k| {
            let f = &k.fields;
            !(f.product == fields.product
                && f.term == fields.term
                && f.feature == fields.feature
                && f.serial == fields.serial
                && f.processor_group == fields.processor_group)
        });
        self.keys.push(InstalledKey {
            fields,
            key: req.key,
            added: today,
        });
        self.sync_definition(def);
        Ok(())
    }

    /// RMVLICKEY: remove the keys matching; `None` matches any. Returns
    /// how many went.
    pub fn remove_keys(
        &mut self,
        p: &ProductId,
        term: Option<&LicTerm>,
        feature: Option<&LoadId>,
        serial: Option<&str>,
        processor_group: Option<&str>,
    ) -> Result<usize, LicError> {
        let before = self.keys.len();
        self.keys.retain(|k| {
            let f = &k.fields;
            !(&f.product == p
                && term.is_none_or(|t| &f.term == t)
                && feature.is_none_or(|x| &f.feature == x)
                && serial.is_none_or(|s| f.serial == s)
                && processor_group.is_none_or(|g| f.processor_group == g))
        });
        match before - self.keys.len() {
            0 => Err(LicError::NoKey),
            n => Ok(n),
        }
    }

    /// DSPLICKEY: the keys of a product (or every product).
    #[must_use]
    pub fn key_rows(&self, p: Option<&ProductId>) -> Vec<KeyRow> {
        let mut rows: Vec<KeyRow> = self
            .keys
            .iter()
            .filter(|k| p.is_none_or(|p| &k.fields.product == p))
            .map(|k| {
                let f = &k.fields;
                KeyRow {
                    product: f.product.to_string(),
                    term: f.term.to_string(),
                    feature: f.feature.to_string(),
                    serial: f.serial.clone(),
                    processor_group: f.processor_group.clone(),
                    usage_limit: nomax(f.usage_limit),
                    expiration: f
                        .expiration
                        .map_or_else(|| "*NONE".to_string(), |d| d.to_string()),
                    vendor_data: f.vendor_data.clone(),
                    key: k.key.lines(),
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            (&a.product, &a.term, &a.feature).cmp(&(&b.product, &b.term, &b.feature))
        });
        rows
    }

    /// WRKLICINF: one row per license information entry.
    #[must_use]
    pub fn info_rows(&self, sys: &SystemInfo, today: Date) -> Vec<InfoRow> {
        let mut rows: Vec<InfoRow> = self
            .info
            .iter()
            .map(|i| {
                let keys = self.valid_keys(i, sys, today);
                let expiration = if i.compliance == Compliance::Keyed {
                    keys.iter().filter_map(|k| k.fields.expiration).max()
                } else {
                    None
                };
                InfoRow {
                    product: i.product.to_string(),
                    term: i.term.to_string(),
                    feature: i.feature.to_string(),
                    compliance: i.compliance.name(),
                    usage_type: i.usage_type.name(),
                    usage_limit: nomax(self.effective_limit(i, sys, today)),
                    usage_count: i.count(),
                    peak: i.peak,
                    threshold: i.threshold.map(|t| t.to_string()).unwrap_or_default(),
                    grace_days: i.grace_days,
                    grace_end: i.grace_start.map(|s| s.plus(i64::from(i.grace_days))),
                    expiration,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            (&a.product, &a.term, &a.feature).cmp(&(&b.product, &b.term, &b.feature))
        });
        rows
    }

    /// CHGLICINF. A keyed limit comes from the key and is not changed.
    pub fn change_info(
        &mut self,
        p: &ProductId,
        term: &LicTerm,
        feature: &LoadId,
        chg: &ChgLicInf,
    ) -> Result<(), LicError> {
        let i = self.info_index(p, term, feature)?;
        let info = &mut self.info[i];
        if let Some(l) = chg.usage_limit {
            if info.compliance == Compliance::Keyed {
                return Err(LicError::KeyedLimit);
            }
            if l.is_some_and(|n| n > 999_999) {
                return Err(LicError::Value("USGLMT"));
            }
            info.usage_limit = l;
        }
        if let Some(t) = chg.threshold {
            if t.is_some_and(|n| n > 999_999) {
                return Err(LicError::Value("THRESHOLD"));
            }
            info.threshold = t;
        }
        if let Some(q) = &chg.msgq {
            info.msgq = q.clone();
        }
        if let Some(l) = chg.log {
            info.log = l;
        }
        Ok(())
    }

    pub(crate) fn info_mut(
        &mut self,
        p: &ProductId,
        rel: Release,
        f: &LoadId,
    ) -> Option<&mut LicenseInfo> {
        self.info
            .iter_mut()
            .find(|i| &i.product == p && &i.feature == f && i.term.covers(rel))
    }
}
