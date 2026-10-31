// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! License use: QLZAREQ (request a use) and QLZARLS (release it).

use release::Release;

use crate::id::{LoadId, ProductId};
use crate::license::LicError;
use crate::license::repo::{LicenseRepo, LicenseUser};
use crate::model::Date;
use crate::model::definition::Compliance;
use crate::system::SystemInfo;

/// Days before a key's expiration that requests carry a warning.
pub const EXPIRY_WARNING_DAYS: i64 = 30;

/// A request for license uses.
#[derive(Clone, Debug)]
pub struct Request {
    /// The product.
    pub product: ProductId,
    /// Its release.
    pub release: Release,
    /// The feature, 5001..9999.
    pub feature: LoadId,
    /// The user name, `*JOB` or `*PROCESSOR` resolved by the caller to
    /// the job's name or the processor.
    pub user: String,
    /// The requester's handle (`LICL0200`); blanks with `LICL0100`.
    pub handle: [u8; 8],
    /// Uses, 1..999999.
    pub uses: u32,
}

/// What a granted request carries along, for the caller to send as
/// messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// `CPF9E17`: past the limit, granted (`*WARNING`).
    OverLimitAdded {
        /// The limit.
        limit: u32,
    },
    /// `CPF9E72`: past the keyed limit, granted until the grace period
    /// ends.
    GraceOverLimit {
        /// When it ends.
        ends: Date,
    },
    /// `CPI9E75`: no key, granted until the grace period ends.
    GraceActive {
        /// When it ends.
        ends: Date,
    },
    /// `CPI9E76`: the key expires soon.
    ExpiresSoon {
        /// On this date.
        on: Date,
    },
    /// The use count reached the threshold; tell the message queue.
    Threshold {
        /// The count.
        count: u32,
    },
}

impl Notice {
    /// The message identifier the caller sends.
    #[must_use]
    pub fn msg_id(&self) -> &'static str {
        match self {
            Notice::OverLimitAdded { .. } => "CPF9E17",
            Notice::GraceOverLimit { .. } => "CPF9E72",
            Notice::GraceActive { .. } => "CPI9E75",
            Notice::ExpiresSoon { .. } => "CPI9E76",
            Notice::Threshold { .. } => "CPI9E19",
        }
    }
}

impl LicenseRepo {
    /// QLZAREQ: grant uses or refuse them.
    pub fn request(
        &mut self,
        req: &Request,
        sys: &SystemInfo,
        today: Date,
    ) -> Result<Vec<Notice>, LicError> {
        if req.user.is_empty() || req.user.len() > 80 {
            return Err(LicError::User);
        }
        if req.uses == 0 || req.uses > 999_999 {
            return Err(LicError::Value("number of uses"));
        }
        let info = self
            .info_for(&req.product, req.release, &req.feature)
            .ok_or(LicError::NoInfo)?
            .clone();
        if let Some(u) = info.users.iter().find(|u| u.name == req.user) {
            return if u.uses == req.uses {
                Ok(Vec::new())
            } else {
                Err(LicError::DifferentUses)
            };
        }
        let mut notices = Vec::new();
        let count = info.count();
        let new_count = count + req.uses;
        let limit = self.effective_limit(&info, sys, today);
        let over = limit.is_some_and(|l| new_count > l);
        let mut grace_start = info.grace_start;
        match info.compliance {
            Compliance::Keyed => {
                let any_key = self.keys.iter().any(|k| {
                    k.fields.product == info.product
                        && k.fields.term == info.term
                        && k.fields.feature == info.feature
                        && k.fields.serial == sys.serial
                });
                let valid = self.valid_keys(&info, sys, today);
                if any_key && valid.is_empty() {
                    return Err(LicError::Expired);
                }
                if valid.is_empty() || over {
                    let start = *grace_start.get_or_insert(today);
                    let ends = start.plus(i64::from(info.grace_days));
                    if today >= ends {
                        return Err(LicError::GraceExpired);
                    }
                    notices.push(if valid.is_empty() {
                        Notice::GraceActive { ends }
                    } else {
                        Notice::GraceOverLimit { ends }
                    });
                } else {
                    grace_start = None;
                    let soon = valid
                        .iter()
                        .filter_map(|k| k.fields.expiration)
                        .filter(|e| e.days() - today.days() <= EXPIRY_WARNING_DAYS)
                        .max();
                    let lasting = valid.iter().any(|k| {
                        k.fields
                            .expiration
                            .is_none_or(|e| e.days() - today.days() > EXPIRY_WARNING_DAYS)
                    });
                    if let (Some(on), false) = (soon, lasting) {
                        notices.push(Notice::ExpiresSoon { on });
                    }
                }
            }
            Compliance::Warning => {
                if let (true, Some(l)) = (over, limit) {
                    notices.push(Notice::OverLimitAdded { limit: l });
                }
            }
            Compliance::OprAction => {
                if over {
                    return Err(LicError::OverLimit);
                }
            }
        }
        if let Some(t) = info.threshold
            && count < t
            && new_count >= t
        {
            notices.push(Notice::Threshold { count: new_count });
        }
        let i = self
            .info_mut(&req.product, req.release, &req.feature)
            .expect("found above");
        i.users.push(LicenseUser {
            name: req.user.clone(),
            handle: req.handle,
            uses: req.uses,
        });
        i.peak = i.peak.max(new_count);
        i.grace_start = grace_start;
        Ok(notices)
    }

    /// QLZARLS: release a user's uses. The handle must match the one
    /// the request gave.
    pub fn release(
        &mut self,
        product: &ProductId,
        rel: Release,
        feature: &LoadId,
        user: &str,
        handle: [u8; 8],
    ) -> Result<(), LicError> {
        let i = self
            .info_mut(product, rel, feature)
            .ok_or(LicError::NoInfo)?;
        let at = i
            .users
            .iter()
            .position(|u| u.name == user && u.handle == handle)
            .ok_or(LicError::User)?;
        i.users.remove(at);
        Ok(())
    }

    /// Release every use a job holds (the job ended).
    pub fn release_all(&mut self, user: &str) {
        for i in &mut self.info {
            i.users.retain(|u| u.name != user);
        }
    }
}
