// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The required-authority table, `required.toml`, read once.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::auth::bits::Auth;

/// What an operation needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Exactly these authorities (none: finding the object is enough).
    Bits(Auth),
    /// Any authority but `*EXCLUDE`.
    Any,
}

#[derive(Deserialize)]
struct Entry {
    need: Vec<String>,
    #[allow(dead_code)]
    ibm: String,
}

#[derive(Deserialize)]
struct File {
    op: BTreeMap<String, Entry>,
}

/// The table as shipped.
pub const SOURCE: &str = include_str!("../../required.toml");

fn table() -> &'static BTreeMap<String, Need> {
    static T: OnceLock<BTreeMap<String, Need>> = OnceLock::new();
    T.get_or_init(|| {
        let f: File = toml::from_str(SOURCE).expect("required.toml parses");
        f.op.into_iter()
            .map(|(k, e)| {
                let need = if e.need.iter().any(|n| n == "*ANY") {
                    Need::Any
                } else {
                    let words: Vec<&str> = e.need.iter().map(String::as_str).collect();
                    Need::Bits(
                        Auth::parse(&words, true)
                            .unwrap_or_else(|w| panic!("required.toml {k}: {w}")),
                    )
                };
                (k, need)
            })
            .collect()
    })
}

/// What `op` needs on an object of type `type_name` (`*FILE`).
#[must_use]
pub fn need(op: &str, type_name: &str) -> Option<Need> {
    let t = table();
    t.get(&format!("{op}@{type_name}"))
        .or_else(|| t.get(op))
        .copied()
}

/// Every operation name in the table, overrides included.
#[must_use]
pub fn names() -> Vec<&'static str> {
    table().keys().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookups() {
        assert_eq!(
            need("obj.delete", "*DTAARA"),
            Some(Need::Bits(Auth(0x0004)))
        );
        assert_eq!(need("obj.delete", "*FILE"), Some(Need::Bits(Auth(0x0005))));
        assert_eq!(need("obj.describe", "*PGM"), Some(Need::Any));
        assert_eq!(need("obj.check", "*PGM"), Some(Need::Bits(Auth::EXCLUDE)));
        assert_eq!(need("nope", "*PGM"), None);
        assert!(names().len() > 20);
    }
}
