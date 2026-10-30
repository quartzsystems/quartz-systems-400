// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The expiry scan (`QSCHKCRT`, and daily in `QSYSCOMM1`): every
//! certificate of the stores that open through their stashes, checked
//! against the thresholds of [`Expiry::THRESHOLDS`] (30, 7 and 1 days,
//! then expired). A warning goes to `QSYSOPR` once per threshold per
//! certificate: the thresholds sent are kept in [`EXPIRY_STATE`] by
//! store and fingerprint, so a renewed certificate starts afresh. The
//! shipped root CAs are not watched; they change with the root set.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{Ctx, Keystore, Note, msgs};
use crate::error::{CertError, Result};
use crate::paths::EXPIRY_STATE;
use crate::roots;
use crate::store::{EntryKind, Expiry};

/// One warning to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// The store as messages name it.
    pub store: String,
    /// The certificate's label.
    pub label: String,
    /// Its subject.
    pub subject: String,
    /// Not valid after, Unix seconds.
    pub not_after: u64,
    /// Whole days left; negative once expired.
    pub days_left: i64,
    /// The threshold crossed: 30, 7, 1, or 0 for expired.
    pub threshold: i64,
}

impl Warning {
    /// The message for `QSYSOPR`.
    pub fn note(&self) -> Note {
        if self.threshold == 0 {
            Note::new(msgs::EXPIRED, &[&self.label, &self.store])
        } else {
            Note::new(
                msgs::EXPIRES,
                &[&self.label, &self.store, &self.days_left.max(0).to_string()],
            )
        }
    }
}

/// What a scan found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Warnings to send, store by store, soonest first.
    pub warnings: Vec<Warning>,
    /// Certificates looked at.
    pub checked: usize,
    /// Stores that could not be opened, with the reason (missing stores
    /// are not listed).
    pub skipped: Vec<(String, String)>,
}

/// The thresholds already warned, by `path|sha256`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    sent: BTreeMap<String, i64>,
}

fn load(ctx: &Ctx<'_>) -> Result<State> {
    match ctx.files.read(EXPIRY_STATE)? {
        Some(b) => serde_json::from_slice(&b)
            .map_err(|e| CertError::Corrupt(format!("expiry state: {e}"))),
        None => Ok(State::default()),
    }
}

/// The stores the daily scan watches: `*SYSTEM` and the local CA's.
pub fn watched() -> Vec<Keystore> {
    vec![Keystore::system(), Keystore::new(super::LOCAL_CA_NAME, "")]
}

/// Scan `stores` at `ctx.now`; the warnings due now, remembered as sent.
pub fn scan(ctx: &Ctx<'_>, stores: &[Keystore]) -> Result<Scan> {
    let old = load(ctx)?;
    let mut state = State::default();
    let mut out = Scan::default();
    for ks in stores {
        if !ctx.files.exists(&ks.path)? {
            continue;
        }
        let store = match ctx.open(ks) {
            Ok(s) => s,
            Err(e) => {
                out.skipped.push((ks.shown().to_string(), e.to_string()));
                // Keep what was sent for a store that did not open.
                let prefix = format!("{}|", ks.path);
                for (k, v) in old.sent.range(prefix.clone()..) {
                    if !k.starts_with(&prefix) {
                        break;
                    }
                    state.sent.insert(k.clone(), *v);
                }
                continue;
            }
        };
        for x in store.expiry(ctx.now)? {
            let e = store.get(&x.label)?;
            let Some(info) = e.info()? else { continue };
            if e.kind == EntryKind::Ca && roots::roots().iter().any(|r| r.info.sha256 == info.sha256) {
                continue;
            }
            out.checked += 1;
            let key = format!("{}|{}", ks.path, info.sha256);
            let Some(t) = Expiry::crossed(&x) else { continue };
            let before = old.sent.get(&key).copied();
            state.sent.insert(key, before.map_or(t, |b| b.min(t)));
            if before.is_none_or(|b| t < b) {
                out.warnings.push(Warning {
                    store: ks.shown().to_string(),
                    label: x.label.clone(),
                    subject: x.subject.clone(),
                    not_after: x.not_after,
                    days_left: x.days_left,
                    threshold: t,
                });
            }
        }
    }
    let bytes = serde_json::to_vec_pretty(&state)
        .map_err(|e| CertError::Corrupt(format!("expiry state: {e}")))?;
    if old.sent != state.sent {
        ctx.files.write(EXPIRY_STATE, &bytes)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::entries::{self, Signer};
    use super::super::testing::*;
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::build::CertSpec;

    fn cert(w: &World, label: &str, days: u32) {
        entries::create(
            &w.ctx(),
            &Keystore::system(),
            label,
            &CertSpec::server("e.example", NOW, days),
            KeyAlgorithm::EcdsaP256,
            &Signer::SelfSigned,
        )
        .unwrap();
    }

    fn sent(w: &World, day: u64) -> Vec<(String, i64)> {
        scan(&w.at(NOW + day * DAY), &watched())
            .unwrap()
            .warnings
            .into_iter()
            .map(|x| (x.label, x.threshold))
            .collect()
    }

    #[test]
    fn each_threshold_warns_once() {
        let w = World::with_system();
        cert(&w, "C40", 40);
        assert!(sent(&w, 0).is_empty());
        assert!(sent(&w, 9).is_empty());
        // 30 days left.
        assert_eq!(sent(&w, 10), [("C40".to_string(), 30)]);
        assert!(sent(&w, 11).is_empty());
        assert!(sent(&w, 32).is_empty());
        // 7 days left.
        assert_eq!(sent(&w, 33), [("C40".to_string(), 7)]);
        // 1 day left.
        assert_eq!(sent(&w, 39), [("C40".to_string(), 1)]);
        assert!(sent(&w, 39).is_empty());
        // Expired.
        assert_eq!(sent(&w, 41), [("C40".to_string(), 0)]);
        assert!(sent(&w, 50).is_empty());
    }

    #[test]
    fn a_scan_that_skips_days_sends_the_nearest_threshold_only() {
        let w = World::with_system();
        cert(&w, "SHORT", 5);
        let first = scan(&w.ctx(), &watched()).unwrap();
        assert_eq!(first.checked, 1);
        assert_eq!(first.warnings.len(), 1);
        let x = &first.warnings[0];
        assert_eq!((x.threshold, x.days_left), (7, 5));
        assert_eq!(x.note(), Note::new(msgs::EXPIRES, &["SHORT", "*SYSTEM", "5"]));
        let gone = scan(&w.at(NOW + 6 * DAY), &watched()).unwrap();
        assert_eq!(gone.warnings[0].note(), Note::new(msgs::EXPIRED, &["SHORT", "*SYSTEM"]));
    }

    #[test]
    fn a_renewed_certificate_starts_afresh_and_roots_are_ignored() {
        let w = World::with_system();
        let mut s = w.ctx().open(&Keystore::system()).unwrap();
        crate::roots::seed(&mut s, NOW).unwrap();
        s.save(&w.files).unwrap();
        cert(&w, "WEB", 20);
        assert_eq!(sent(&w, 0), [("WEB".to_string(), 30)]);
        entries::remove(&w.ctx(), &Keystore::system(), "WEB").unwrap();
        cert(&w, "WEB", 25);
        assert_eq!(sent(&w, 0), [("WEB".to_string(), 30)]);
        assert_eq!(scan(&w.ctx(), &watched()).unwrap().checked, 1);
    }

    #[test]
    fn a_store_that_does_not_open_is_reported() {
        let w = World::with_system();
        cert(&w, "X", 3);
        assert_eq!(sent(&w, 0).len(), 1);
        let no_key = Ctx { sealer: None, ..w.ctx() };
        let s = scan(&no_key, &watched()).unwrap();
        assert_eq!(s.skipped.len(), 1);
        // What was sent survives the failed scan.
        assert!(sent(&w, 0).is_empty());
    }
}
