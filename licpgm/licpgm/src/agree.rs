// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Software agreements: the accept or decline answer per product per
//! release, which WRKSFWAGR shows and the GO LICPGM install flow needs.

use std::collections::BTreeMap;

use release::Release;
use thiserror::Error;

use crate::id::{self, ProductId};
use crate::model::Date;
use crate::payload::PayloadError;
use crate::payload::codec::{Reader, Writer, get_text, put_text};

/// The answer given to one agreement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Accepted, by whom and when.
    Accepted {
        /// The user profile.
        user: String,
        /// The date.
        date: Date,
    },
    /// Declined, by whom and when.
    Declined {
        /// The user profile.
        user: String,
        /// The date.
        date: Date,
    },
}

/// Every agreement answer on the system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgreementBook {
    answers: BTreeMap<(ProductId, Release), Answer>,
}

/// An agreement that stops an install.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AgreementError {
    /// No answer yet.
    #[error("the software agreement of {0} {1} has not been accepted")]
    NotAnswered(ProductId, Release),
    /// Declined.
    #[error("the software agreement of {0} {1} was declined")]
    Declined(ProductId, Release),
}

/// One WRKSFWAGR row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgreementRow {
    /// The product.
    pub product: ProductId,
    /// Its release.
    pub release: Release,
    /// Our description.
    pub text: String,
    /// `*ACCEPTED`, `*DECLINED` or `*NONE`.
    pub status: &'static str,
}

impl AgreementBook {
    /// Record acceptance.
    pub fn accept(&mut self, p: &ProductId, r: Release, user: &str, date: Date) {
        self.answers.insert(
            (p.clone(), r),
            Answer::Accepted {
                user: user.into(),
                date,
            },
        );
    }

    /// Record a decline.
    pub fn decline(&mut self, p: &ProductId, r: Release, user: &str, date: Date) {
        self.answers.insert(
            (p.clone(), r),
            Answer::Declined {
                user: user.into(),
                date,
            },
        );
    }

    /// Forget the answer (DLTLICPGM).
    pub fn forget(&mut self, p: &ProductId, r: Release) {
        self.answers.remove(&(p.clone(), r));
    }

    /// The answer, if any.
    #[must_use]
    pub fn answer(&self, p: &ProductId, r: Release) -> Option<&Answer> {
        self.answers.get(&(p.clone(), r))
    }

    /// Ok when accepted.
    pub fn require(&self, p: &ProductId, r: Release) -> Result<(), AgreementError> {
        match self.answer(p, r) {
            Some(Answer::Accepted { .. }) => Ok(()),
            Some(Answer::Declined { .. }) => Err(AgreementError::Declined(p.clone(), r)),
            None => Err(AgreementError::NotAnswered(p.clone(), r)),
        }
    }

    /// The WRKSFWAGR rows for products with agreements.
    #[must_use]
    pub fn rows(&self, products: &[(ProductId, Release, String)]) -> Vec<AgreementRow> {
        products
            .iter()
            .map(|(p, r, t)| AgreementRow {
                product: p.clone(),
                release: *r,
                text: t.clone(),
                status: match self.answer(p, *r) {
                    Some(Answer::Accepted { .. }) => "*ACCEPTED",
                    Some(Answer::Declined { .. }) => "*DECLINED",
                    None => "*NONE",
                },
            })
            .collect()
    }

    /// Encode: a count, then per answer the product (7), release (6),
    /// kind (1: 1 accepted, 2 declined), date `YYYYMMDD` (4) and the
    /// user as a length-prefixed text.
    pub fn encode(&self) -> Result<Vec<u8>, PayloadError> {
        let mut w = Writer::default();
        w.bytes(b"QSAGREE1");
        w.u32(u32::try_from(self.answers.len()).map_err(|_| PayloadError::TooMany("answers"))?);
        for ((p, r), a) in &self.answers {
            w.char(p.as_str(), 7, "product")?;
            w.char(&id::release_text(*r), 6, "release")?;
            let (k, user, date) = match a {
                Answer::Accepted { user, date } => (1, user, date),
                Answer::Declined { user, date } => (2, user, date),
            };
            w.u8(k);
            w.u32(date.ymd());
            put_text(&mut w, user, "user")?;
        }
        Ok(w.buf)
    }

    /// Decode what [`AgreementBook::encode`] wrote.
    pub fn decode(b: &[u8]) -> Result<Self, PayloadError> {
        let mut r = Reader::new(b);
        if r.take(8)? != b"QSAGREE1" {
            return Err(PayloadError::Magic);
        }
        let n = r.u32()?;
        let mut answers = BTreeMap::new();
        for _ in 0..n {
            let p: ProductId = r
                .char(7)?
                .parse()
                .map_err(|_| PayloadError::Bad("product"))?;
            let rel = id::parse_release(&r.char(6)?).map_err(|_| PayloadError::Bad("release"))?;
            let k = r.u8()?;
            let date = Date::from_ymd(r.u32()?).ok_or(PayloadError::Bad("date"))?;
            let user = get_text(&mut r, "user")?;
            let a = match k {
                1 => Answer::Accepted { user, date },
                2 => Answer::Declined { user, date },
                _ => return Err(PayloadError::Bad("kind")),
            };
            answers.insert((p, rel), a);
        }
        Ok(Self { answers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_decline_require() {
        let p: ProductId = "QS00WDS".parse().unwrap();
        let r = release::RELEASE;
        let d = Date::new(2026, 10, 30).unwrap();
        let mut b = AgreementBook::default();
        assert_eq!(
            b.require(&p, r),
            Err(AgreementError::NotAnswered(p.clone(), r))
        );
        b.decline(&p, r, "QSECOFR", d);
        assert!(matches!(
            b.require(&p, r),
            Err(AgreementError::Declined(..))
        ));
        b.accept(&p, r, "QSECOFR", d);
        assert!(b.require(&p, r).is_ok());
        let rows = b.rows(&[(p.clone(), r, "x".into())]);
        assert_eq!(rows[0].status, "*ACCEPTED");
        assert_eq!(AgreementBook::decode(&b.encode().unwrap()).unwrap(), b);
    }
}
