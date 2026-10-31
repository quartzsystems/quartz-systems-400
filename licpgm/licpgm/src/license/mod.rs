// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! License management: keys, license information, use.

pub mod key;
pub mod keyfile;
pub mod repo;
pub mod usage;

use thiserror::Error;

use crate::id::{LicTerm, LoadId};
use crate::model::definition::{Compliance, UsageType};
use crate::model::{Date, QualName};
use crate::payload::PayloadError;
use crate::payload::codec::{Reader, Writer, get_text, put_text};
use key::{KeyError, KeyFields, LicenseKey};
use repo::{InstalledKey, LicenseInfo, LicenseRepo, LicenseUser};

/// A license operation that cannot be done.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum LicError {
    /// No license information for the product, term and feature.
    #[error("there is no license information for the product, term and feature")]
    NoInfo,
    /// The key is for another system.
    #[error("the license key is for another system serial number")]
    SerialMismatch,
    /// The key is for another processor group.
    #[error("the license key is for another processor group")]
    ProcessorGroup,
    /// The key has expired.
    #[error("the license key has expired")]
    Expired,
    /// The key does not check out.
    #[error(transparent)]
    Key(KeyError),
    /// No public key to check keys with.
    #[error("the product has no license public key to check keys with")]
    NoPublicKey,
    /// No key matched.
    #[error("no license key matches")]
    NoKey,
    /// A keyed usage limit comes from the key.
    #[error("the usage limit of a keyed product comes from its license key")]
    KeyedLimit,
    /// A value out of range.
    #[error("the {0} value is not valid")]
    Value(&'static str),
    /// The user is not valid or holds nothing under that handle.
    #[error("the license user is not valid")]
    User,
    /// The user holds a different number of uses.
    #[error("the user already holds a different number of uses")]
    DifferentUses,
    /// Past the usage limit, `*OPRACTION`.
    #[error("the usage limit would be exceeded")]
    OverLimit,
    /// Past the grace period.
    #[error("the grace period has ended")]
    GraceExpired,
}

impl LicError {
    /// The message identifier an API sends for it.
    #[must_use]
    pub fn msg_id(&self) -> &'static str {
        match self {
            LicError::NoInfo => "CPF9E12",
            LicError::SerialMismatch | LicError::ProcessorGroup | LicError::Key(_) => "CPF9E78",
            LicError::Expired => "CPF9E73",
            LicError::NoPublicKey => "CPF9E15",
            LicError::NoKey => "CPF9E11",
            LicError::KeyedLimit | LicError::Value(_) => "CPF9E15",
            LicError::User => "CPF9E91",
            LicError::DifferentUses => "CPF9E79",
            LicError::OverLimit => "CPF9E18",
            LicError::GraceExpired => "CPF9E71",
        }
    }
}

fn usage_code(u: UsageType) -> u8 {
    match u {
        UsageType::Concurrent => 0,
        UsageType::Registered => 1,
        UsageType::Processor => 2,
    }
}

fn compliance_code(c: Compliance) -> u8 {
    match c {
        Compliance::OprAction => 0,
        Compliance::Warning => 1,
        Compliance::Keyed => 2,
    }
}

impl LicenseRepo {
    /// Encode for the integrator's object (`docs/LICPGM.md` "License
    /// repository").
    pub fn encode(&self) -> Result<Vec<u8>, PayloadError> {
        let mut w = Writer::default();
        w.bytes(b"QSLICRP1");
        w.u32(u32::try_from(self.keys.len()).map_err(|_| PayloadError::TooMany("keys"))?);
        for k in &self.keys {
            let f = &k.fields;
            w.char(f.product.as_str(), 7, "product")?;
            w.char(f.term.as_str(), 6, "term")?;
            w.char(f.feature.as_str(), 4, "feature")?;
            w.char(&f.serial, 8, "serial")?;
            w.char(&f.processor_group, 4, "processor_group")?;
            w.u32(f.usage_limit.unwrap_or(u32::MAX));
            w.u32(f.expiration.map_or(0, Date::ymd));
            w.char(&f.vendor_data, 8, "vendor_data")?;
            w.u32(k.added.ymd());
            w.bytes(k.key.signature());
        }
        w.u32(u32::try_from(self.info.len()).map_err(|_| PayloadError::TooMany("info"))?);
        for i in &self.info {
            w.char(i.product.as_str(), 7, "product")?;
            w.char(i.term.as_str(), 6, "term")?;
            w.char(i.feature.as_str(), 4, "feature")?;
            w.u8(usage_code(i.usage_type));
            w.u8(compliance_code(i.compliance));
            w.u32(i.usage_limit.unwrap_or(u32::MAX));
            w.u32(i.threshold.unwrap_or(u32::MAX));
            w.u16(i.grace_days);
            w.u32(i.grace_start.map_or(0, Date::ymd));
            w.char(&i.msgq.name, 10, "msgq")?;
            w.char(&i.msgq.lib, 10, "msgq")?;
            w.u8(u8::from(i.log));
            w.u32(i.peak);
            w.u32(u32::try_from(i.users.len()).map_err(|_| PayloadError::TooMany("users"))?);
            for u in &i.users {
                put_text(&mut w, &u.name, "user")?;
                w.bytes(&u.handle);
                w.u32(u.uses);
            }
        }
        Ok(w.buf)
    }

    /// Decode what [`LicenseRepo::encode`] wrote.
    pub fn decode(b: &[u8]) -> Result<Self, PayloadError> {
        let mut r = Reader::new(b);
        if r.take(8)? != b"QSLICRP1" {
            return Err(PayloadError::Magic);
        }
        let opt = |v: u32| (v != u32::MAX).then_some(v);
        let date = |v: u32| -> Result<Option<Date>, PayloadError> {
            if v == 0 {
                Ok(None)
            } else {
                Date::from_ymd(v).map(Some).ok_or(PayloadError::Bad("date"))
            }
        };
        let mut keys = Vec::new();
        for _ in 0..r.u32()? {
            let product = r
                .char(7)?
                .parse()
                .map_err(|_| PayloadError::Bad("product"))?;
            let term: LicTerm = r.char(6)?.parse().map_err(|_| PayloadError::Bad("term"))?;
            let feature = LoadId::feature(&r.char(4)?).map_err(|_| PayloadError::Bad("feature"))?;
            let serial = r.char(8)?;
            let processor_group = r.char(4)?;
            let usage_limit = opt(r.u32()?);
            let expiration = date(r.u32()?)?;
            let vendor_data = r.char(8)?;
            let added = date(r.u32()?)?.ok_or(PayloadError::Bad("added"))?;
            let sig = r.take(64)?;
            let k = LicenseKey::from_signature(sig.try_into().expect("64 bytes"));
            keys.push(InstalledKey {
                fields: KeyFields {
                    product,
                    term,
                    feature,
                    serial,
                    processor_group,
                    usage_limit,
                    expiration,
                    vendor_data,
                },
                key: k,
                added,
            });
        }
        let mut info = Vec::new();
        for _ in 0..r.u32()? {
            let product = r
                .char(7)?
                .parse()
                .map_err(|_| PayloadError::Bad("product"))?;
            let term = r.char(6)?.parse().map_err(|_| PayloadError::Bad("term"))?;
            let feature = LoadId::feature(&r.char(4)?).map_err(|_| PayloadError::Bad("feature"))?;
            let usage_type = match r.u8()? {
                0 => UsageType::Concurrent,
                1 => UsageType::Registered,
                2 => UsageType::Processor,
                _ => return Err(PayloadError::Bad("usage_type")),
            };
            let compliance = match r.u8()? {
                0 => Compliance::OprAction,
                1 => Compliance::Warning,
                2 => Compliance::Keyed,
                _ => return Err(PayloadError::Bad("compliance")),
            };
            let usage_limit = opt(r.u32()?);
            let threshold = opt(r.u32()?);
            let grace_days = r.u16()?;
            let grace_start = date(r.u32()?)?;
            let qn = r.char(10)?;
            let ql = r.char(10)?;
            let log = r.u8()? != 0;
            let peak = r.u32()?;
            let mut users = Vec::new();
            for _ in 0..r.u32()? {
                let name = get_text(&mut r, "user")?;
                let handle = r.take(8)?.try_into().expect("8 bytes");
                let uses = r.u32()?;
                users.push(LicenseUser { name, handle, uses });
            }
            info.push(LicenseInfo {
                product,
                term,
                feature,
                usage_type,
                compliance,
                usage_limit,
                threshold,
                grace_days,
                grace_start,
                msgq: QualName::new(&qn, &ql),
                log,
                peak,
                users,
            });
        }
        if r.pos() != b.len() {
            return Err(PayloadError::Trailing(b.len() - r.pos()));
        }
        Ok(Self { keys, info })
    }
}

impl LicenseKey {
    /// A key from its signature bytes (stored keys).
    #[must_use]
    pub fn from_signature(sig: [u8; 64]) -> Self {
        key::from_sig(sig)
    }
}
