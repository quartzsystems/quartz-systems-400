// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The license key file ADDLICKEY reads with `LICKEYINP(*LICKEYFILE)`:
//! TOML, one `[[key]]` table per key.

use serde::Deserialize;
use std::fmt::Write as _;
use thiserror::Error;

use crate::id::{LoadId, ProductId};
use crate::license::key::{KeyFields, LicenseKey};
use crate::model::Date;
use crate::pkg::schema::NumOrText;

/// One key and its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyRecord {
    /// The fields.
    pub fields: KeyFields,
    /// The key.
    pub key: LicenseKey,
}

/// A key file that does not read.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum KeyFileError {
    /// Not the file's TOML shape.
    #[error("not a license key file: {0}")]
    Syntax(String),
    /// A key with a bad field.
    #[error("key {index}: {message}")]
    Key {
        /// Which `[[key]]`, from 1.
        index: usize,
        /// What is wrong.
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    key: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    product: String,
    term: String,
    feature: String,
    serial: String,
    #[serde(default = "any")]
    processor_group: String,
    usage_limit: NumOrText,
    #[serde(default = "none")]
    expiration: String,
    #[serde(default)]
    vendor_data: String,
    key: String,
}

fn any() -> String {
    "*ANY".into()
}

fn none() -> String {
    "*NONE".into()
}

/// Read a key file.
pub fn parse(text: &str) -> Result<Vec<KeyRecord>, KeyFileError> {
    let f: File =
        toml::from_str(text).map_err(|e| KeyFileError::Syntax(e.message().to_string()))?;
    f.key
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let bad = |m: String| KeyFileError::Key {
                index: i + 1,
                message: m,
            };
            let product: ProductId = e
                .product
                .parse()
                .map_err(|_| bad("product is not valid".into()))?;
            let term = e
                .term
                .parse()
                .map_err(|_| bad("term is not valid".into()))?;
            let feature =
                LoadId::feature(&e.feature).map_err(|_| bad("feature is not valid".into()))?;
            let usage_limit = match e.usage_limit.text().as_str() {
                "*NOMAX" => None,
                t => Some(
                    t.parse()
                        .map_err(|_| bad("usage_limit is not valid".into()))?,
                ),
            };
            let expiration = match e.expiration.as_str() {
                "*NONE" => None,
                d => Some(Date::parse(d).ok_or_else(|| bad("expiration is not a date".into()))?),
            };
            let fields = KeyFields {
                product,
                term,
                feature,
                serial: e.serial,
                processor_group: e.processor_group,
                usage_limit,
                expiration,
                vendor_data: e.vendor_data,
            };
            fields.validate().map_err(|x| bad(x.to_string()))?;
            let key = LicenseKey::parse(&e.key).map_err(|x| bad(x.to_string()))?;
            Ok(KeyRecord { fields, key })
        })
        .collect()
}

/// Write a key file.
#[must_use]
pub fn write(records: &[KeyRecord]) -> String {
    let mut s = String::from("# Quartz Systems/400 license keys.\n");
    for r in records {
        let f = &r.fields;
        let _ = write!(
            s,
            "\n[[key]]\nproduct = \"{}\"\nterm = \"{}\"\nfeature = \"{}\"\nserial = \"{}\"\nprocessor_group = \"{}\"\n",
            f.product, f.term, f.feature, f.serial, f.processor_group
        );
        match f.usage_limit {
            Some(n) => {
                let _ = writeln!(s, "usage_limit = {n}");
            }
            None => s.push_str("usage_limit = \"*NOMAX\"\n"),
        }
        let exp = f
            .expiration
            .map_or_else(|| "*NONE".to_string(), |d| d.to_string());
        let _ = writeln!(s, "expiration = \"{exp}\"");
        let vd = f.vendor_data.replace('\\', "\\\\").replace('"', "\\\"");
        let _ = writeln!(s, "vendor_data = \"{vd}\"");
        let _ = writeln!(s, "key = \"\"\"\n{}\"\"\"", r.key.lines().join("\n") + "\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::key::generate;
    use ed25519_dalek::SigningKey;

    #[test]
    fn write_then_parse() {
        let sk = SigningKey::from_bytes(&[3; 32]);
        let fields = KeyFields {
            product: "QS00WDS".parse().unwrap(),
            term: "V1".parse().unwrap(),
            feature: LoadId::feature("5102").unwrap(),
            serial: "QS40001".into(),
            processor_group: "*ANY".into(),
            usage_limit: None,
            expiration: None,
            vendor_data: "AB\"C".into(),
        };
        let key = generate(&sk, &fields).unwrap();
        let recs = vec![KeyRecord { fields, key }];
        let text = write(&recs);
        assert_eq!(parse(&text).unwrap(), recs);
        assert!(matches!(
            parse("[[key]]\nproduct = 1"),
            Err(KeyFileError::Syntax(_))
        ));
        let bad = text.replace("QS40001", "qs-1");
        assert!(matches!(
            parse(&bad),
            Err(KeyFileError::Key { index: 1, .. })
        ));
    }
}
