// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The whole configuration: the attributes and the shares, as the two
//! files hold them.

use std::collections::BTreeMap;

use super::attrs::Attributes;
use super::share::{Share, shipped};

/// The attributes and the shares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Current and pending attributes.
    pub attrs: Attributes,
    /// The shares by name, upper case.
    pub shares: BTreeMap<String, Share>,
    /// Records of the attribute file this release does not know, kept.
    pub unknown: Vec<(u16, Vec<u8>)>,
    /// Records of the share file outside any share, kept.
    pub unknown_shares: Vec<(u16, Vec<u8>)>,
}

impl Config {
    /// A first configuration: IBM's defaults for server `name` and the
    /// shipped shares.
    #[must_use]
    pub fn shipped(name: &str) -> Self {
        Self {
            attrs: Attributes::new(name),
            shares: shipped().into_iter().map(|s| (s.name.clone(), s)).collect(),
            unknown: Vec::new(),
            unknown_shares: Vec::new(),
        }
    }

    /// The share named `name` (case not significant).
    #[must_use]
    pub fn share(&self, name: &str) -> Option<&Share> {
        self.shares.get(&name.to_ascii_uppercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_has_qibm_and_defaults() {
        let c = Config::shipped("QS400");
        assert!(c.share("qibm").is_some());
        assert_eq!(c.attrs.current.server_name, "QS400");
    }
}
