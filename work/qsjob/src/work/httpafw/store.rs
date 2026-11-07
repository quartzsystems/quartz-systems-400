// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product's stores: user indexes `QUSRSYS/QZXWPOL` (policies) and
//! `QUSRSYS/QZXWPRF` (preferences), made with `QS00AFW` and owned by
//! `QTMHHTTP` (`lic/osobjects`' products.rs), read and written as the
//! job. An entry is the key, a profile name in 10 bytes and a function
//! or preference key in 20, then the value.

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

fn key(who: &str, what: &str) -> Vec<u8> {
    format!("{who:<WHO$}{what:<20}").into_bytes()
}

/// `who`'s entries: key and value.
fn read(link: &Link, index: &'static str, who: &str) -> Vec<(String, String)> {
    let lo = key(who, "");
    let mut hi = format!("{who:<WHO$}").into_bytes();
    hi.extend(std::iter::repeat_n(b'~', KEY - WHO));
    link.job(move |m| {
        m.ask(|l| l.usridx_find(LIB.into(), index.into(), 8, lo, hi, 10_000, false))
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.len() >= KEY)
            .map(|e| {
                let k = String::from_utf8_lossy(&e[WHO..KEY]).trim_end().to_string();
                (k, String::from_utf8_lossy(&e[KEY..]).into_owned())
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Replace `who`'s entries with `values`.
fn write(link: &Link, index: &'static str, who: &str, values: &[(String, String)]) -> Result<(), Message> {
    let lo = key(who, "");
    let mut hi = format!("{who:<WHO$}").into_bytes();
    hi.extend(std::iter::repeat_n(b'~', KEY - WHO));
    let entries: Vec<Vec<u8>> = values
        .iter()
        .map(|(k, v)| {
            let mut e = key(who, k);
            e.extend_from_slice(v.as_bytes());
            e
        })
        .collect();
    let r = link.job(move |m| {
        m.ask(|l| l.usridx_find(LIB.into(), index.into(), 8, lo, hi, 10_000, true))
            .map_err(|e| e.msgid)?;
        if entries.is_empty() {
            return Ok(());
        }
        m.ask(|l| l.usridx_add(LIB.into(), index.into(), entries, true))
            .map(|_| ())
            .map_err(|e| e.msgid)
    });
    r.unwrap_or_else(|| Err("CPF9897".into())).map_err(|id: String| {
        Message::new(Kind::Error, &id, &format!("The Access for Web store {LIB}/{index} could not be changed."))
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
        assert_eq!(key("WEBUSER", "db.write").len(), KEY);
        assert_eq!(&key("WEBUSER", "db.write")[..12], b"WEBUSER   db");
        assert!(fits("policies.change") && !fits("a.very.long.preference.key"));
    }
}
