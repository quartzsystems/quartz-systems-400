// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product's stores: user indexes `QUSRSYS/QZXWPOL` (policies) and
//! `QUSRSYS/QZXWPRF` (preferences), made with `QS00AFW`, owned by the
//! system and `*PUBLIC *USE`; the job reads and changes them through
//! the storage service (`web_store`, which only this job may send).

use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::env::prefs::PrefStore;
use qsweb::msg::{Kind, Message};
use stg::Client;

use super::Link;

/// The stores' library.
const LIB: &str = "QUSRSYS";
/// The policies.
const POLICIES: &str = "QZXWPOL";
/// The preferences.
const PREFS: &str = "QZXWPRF";
/// The profile part of a key.
const WHO: usize = 10;
/// The whole key.
const KEY: usize = 30;

/// `who`'s entries: key and value.
fn read(link: &Link, index: &'static str, who: &str) -> Vec<(String, String)> {
    let w = who.to_string();
    link.job(move |m| {
        m.ask(|l| l.web_store(index.into(), w, None))
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// Replace `who`'s entries with `values`.
fn write(
    link: &Link,
    index: &'static str,
    who: &str,
    values: &[(String, String)],
) -> Result<(), Message> {
    let (w, v) = (who.to_string(), values.to_vec());
    let r = link.job(move |m| m.ask(|l| l.web_store(index.into(), w, Some(v))).map(|_| ()));
    r.unwrap_or_else(|| Err("CPF9897".into()))
        .map_err(|id: String| {
            Message::new(
                Kind::Error,
                &id,
                &format!("The Access for Web store {LIB}/{index} could not be changed."),
            )
        })
}

/// A store's key: up to 20 characters of ASCII.
fn fits(k: &str) -> bool {
    k.len() <= KEY - WHO && k.is_ascii()
}

/// Policies in `QUSRSYS/QZXWPOL`.
pub struct LivePolicy {
    /// The job's thread.
    pub link: Link,
}

impl PolicyStore for LivePolicy {
    fn settings(&self, subject: &str) -> Vec<(String, Setting)> {
        read(&self.link, POLICIES, subject)
            .into_iter()
            .filter_map(|(k, v)| Setting::from_code(&v).map(|s| (k, s)))
            .collect()
    }

    fn put(&self, subject: &str, entries: &[(String, Setting)]) -> Result<(), Message> {
        let v: Vec<(String, String)> = entries
            .iter()
            .filter(|(k, _)| fits(k))
            .map(|(k, s)| (k.clone(), s.code().to_string()))
            .collect();
        write(&self.link, POLICIES, subject, &v)
    }
}

/// Preferences in `QUSRSYS/QZXWPRF`.
pub struct LivePrefs {
    /// The job's thread.
    pub link: Link,
}

impl PrefStore for LivePrefs {
    fn get(&self, user: &str) -> Vec<(String, String)> {
        read(&self.link, PREFS, user)
    }

    fn put(&self, user: &str, values: &[(String, String)]) -> Result<(), Message> {
        let v: Vec<(String, String)> = values
            .iter()
            .filter(|(k, v)| fits(k) && v.len() <= 98)
            .cloned()
            .collect();
        write(&self.link, PREFS, user, &v)
    }

    fn schemas(&self, user: &str) -> Vec<String> {
        // The libraries the user may see: QGPL and those not the
        // system's own (Q names).
        self.link.read(user, |m| {
            let mut v: Vec<String> = m
                .ask(|l| l.list_objects("QSYS".into(), "*ALL".into(), "*LIB".into()))
                .into_iter()
                .map(|o| o.name)
                .filter(|n| n == "QGPL" || !n.starts_with('Q'))
                .collect();
            v.sort();
            v.dedup();
            v
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert!(fits("policies.change") && !fits("a.very.long.preference.key"));
    }
}
