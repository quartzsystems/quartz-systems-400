// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system's TLS policy from `QSSLPCL`, `QSSLCSL` and `QSSLCSLCTL`,
//! and the rustls provider and versions it allows.

use std::sync::Arc;

use rustls::SupportedProtocolVersion;
use rustls::crypto::CryptoProvider;

use crate::error::{Result, TlsError};
use crate::suites::{self, IbmSuite};

/// A protocol version as `QSSLPCL` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Protocol {
    /// `*SSLV2` (never enabled).
    Ssl2,
    /// `*SSLV3` (never enabled).
    Ssl3,
    /// `*TLSV1` (never enabled).
    Tls10,
    /// `*TLSV1.1` (never enabled).
    Tls11,
    /// `*TLSV1.2`.
    Tls12,
    /// `*TLSV1.3`.
    Tls13,
}

impl Protocol {
    /// The `QSSLPCL` name.
    pub fn name(self) -> &'static str {
        match self {
            Protocol::Ssl2 => "*SSLV2",
            Protocol::Ssl3 => "*SSLV3",
            Protocol::Tls10 => "*TLSV1",
            Protocol::Tls11 => "*TLSV1.1",
            Protocol::Tls12 => "*TLSV1.2",
            Protocol::Tls13 => "*TLSV1.3",
        }
    }

    /// Parse a `QSSLPCL` element.
    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*SSLV2" => Some(Protocol::Ssl2),
            "*SSLV3" => Some(Protocol::Ssl3),
            "*TLSV1" | "*TLSV1.0" => Some(Protocol::Tls10),
            "*TLSV1.1" => Some(Protocol::Tls11),
            "*TLSV1.2" => Some(Protocol::Tls12),
            "*TLSV1.3" => Some(Protocol::Tls13),
            _ => None,
        }
    }

    /// The engine implements it.
    pub fn supported(self) -> bool {
        matches!(self, Protocol::Tls12 | Protocol::Tls13)
    }

    /// The rustls version.
    pub fn rustls(self) -> Option<&'static SupportedProtocolVersion> {
        match self {
            Protocol::Tls12 => Some(&rustls::version::TLS12),
            Protocol::Tls13 => Some(&rustls::version::TLS13),
            _ => None,
        }
    }

    /// From a negotiated rustls version.
    pub fn of_rustls(v: rustls::ProtocolVersion) -> Option<Self> {
        match v {
            rustls::ProtocolVersion::TLSv1_3 => Some(Protocol::Tls13),
            rustls::ProtocolVersion::TLSv1_2 => Some(Protocol::Tls12),
            rustls::ProtocolVersion::TLSv1_1 => Some(Protocol::Tls11),
            rustls::ProtocolVersion::TLSv1_0 => Some(Protocol::Tls10),
            rustls::ProtocolVersion::SSLv3 => Some(Protocol::Ssl3),
            rustls::ProtocolVersion::SSLv2 => Some(Protocol::Ssl2),
            _ => None,
        }
    }
}

/// What `*OPSYS` means for protocols.
pub const OPSYS_PROTOCOLS: [Protocol; 2] = [Protocol::Tls13, Protocol::Tls12];

/// The effective policy.
#[derive(Clone, Debug)]
pub struct TlsPolicy {
    /// Enabled and supported protocols, newest first.
    pub protocols: Vec<Protocol>,
    /// Enabled and supported suites in preference order.
    pub suites: Vec<&'static IbmSuite>,
    /// `QSSLPCL` entries the engine cannot enable.
    pub unsupported_protocols: Vec<Protocol>,
    /// `QSSLCSL` entries not recognized or not implemented.
    pub unmapped_suites: Vec<String>,
}

impl Default for TlsPolicy {
    fn default() -> Self {
        Self::opsys()
    }
}

impl TlsPolicy {
    /// The `*OPSYS` policy: TLS 1.3 and 1.2, every supported suite.
    pub fn opsys() -> Self {
        Self {
            protocols: OPSYS_PROTOCOLS.to_vec(),
            suites: suites::opsys(),
            unsupported_protocols: Vec::new(),
            unmapped_suites: Vec::new(),
        }
    }

    /// From the system values: `QSSLPCL` (list), `QSSLCSL` (list) and
    /// `QSSLCSLCTL` (`*OPSYS` or `*USRDFN`). `QSSLCSL` only counts when
    /// `QSSLCSLCTL` is `*USRDFN`, as on IBM i.
    pub fn from_sysvals(pcl: &[&str], csl: &[&str], cslctl: &str) -> Self {
        let mut p = Self::opsys();
        let pcl: Vec<&str> = pcl
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if !pcl.is_empty() && !pcl.iter().any(|s| s.eq_ignore_ascii_case("*OPSYS")) {
            p.protocols.clear();
            for s in &pcl {
                match Protocol::from_name(s) {
                    Some(v) if v.supported() => {
                        if !p.protocols.contains(&v) {
                            p.protocols.push(v);
                        }
                    }
                    Some(v) => p.unsupported_protocols.push(v),
                    None => {}
                }
            }
            p.protocols.sort_by(|a, b| b.cmp(a));
        }
        if cslctl.trim().eq_ignore_ascii_case("*USRDFN") {
            let csl: Vec<&str> = csl
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !csl.is_empty() && !csl.iter().any(|s| s.eq_ignore_ascii_case("*OPSYS")) {
                p.suites.clear();
                for s in &csl {
                    match suites::by_name(s) {
                        Some(x) if x.supported() => {
                            if !p.suites.iter().any(|y| y.code == x.code) {
                                p.suites.push(x);
                            }
                        }
                        _ => p.unmapped_suites.push((*s).to_string()),
                    }
                }
            }
        }
        p
    }

    /// A policy restricted to `protocols` (an application's own setting
    /// within the system's).
    #[must_use]
    pub fn restricted_to(&self, protocols: &[Protocol]) -> Self {
        let mut p = self.clone();
        p.protocols.retain(|v| protocols.contains(v));
        p
    }

    /// A policy restricted to `codes` (an application's own cipher list).
    #[must_use]
    pub fn with_suites(&self, codes: &[u16]) -> Self {
        let mut p = self.clone();
        p.suites.retain(|s| codes.contains(&s.code));
        p
    }

    /// The suites usable with the enabled protocols.
    pub fn usable_suites(&self) -> Vec<&'static IbmSuite> {
        let tls13 = self.protocols.contains(&Protocol::Tls13);
        let tls12 = self.protocols.contains(&Protocol::Tls12);
        self.suites
            .iter()
            .copied()
            .filter(|s| if s.tls13 { tls13 } else { tls12 })
            .collect()
    }

    /// The rustls versions, or `NothingEnabled`.
    pub fn versions(&self) -> Result<Vec<&'static SupportedProtocolVersion>> {
        let v: Vec<_> = self.protocols.iter().filter_map(|p| p.rustls()).collect();
        if v.is_empty() {
            return Err(TlsError::NothingEnabled("protocol"));
        }
        Ok(v)
    }

    /// A graviola provider with only the policy's suites.
    pub fn provider(&self) -> Result<Arc<CryptoProvider>> {
        let mut p = rustls_graviola::default_provider();
        let usable = self.usable_suites();
        p.cipher_suites = usable.iter().filter_map(|s| s.rustls.copied()).collect();
        if p.cipher_suites.is_empty() {
            return Err(TlsError::NothingEnabled("cipher suite"));
        }
        Ok(Arc::new(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opsys_and_restrictions() {
        let p = TlsPolicy::from_sysvals(&["*OPSYS"], &["*OPSYS"], "*OPSYS");
        assert_eq!(p.protocols, vec![Protocol::Tls13, Protocol::Tls12]);
        assert_eq!(p.suites.len(), 9);
        let p = TlsPolicy::from_sysvals(&["*TLSV1.2", "*TLSV1"], &[], "*OPSYS");
        assert_eq!(p.protocols, vec![Protocol::Tls12]);
        assert_eq!(p.unsupported_protocols, vec![Protocol::Tls10]);
        assert!(p.usable_suites().iter().all(|s| !s.tls13));
        let p = TlsPolicy::from_sysvals(
            &["*TLSV1.3"],
            &["*AES_256_GCM_SHA384", "*RSA_RC4_128_SHA", "*BOGUS"],
            "*USRDFN",
        );
        assert_eq!(p.suites.len(), 1);
        assert_eq!(p.unmapped_suites, vec!["*RSA_RC4_128_SHA", "*BOGUS"]);
        assert_eq!(p.provider().unwrap().cipher_suites.len(), 1);
        // QSSLCSL is ignored unless QSSLCSLCTL is *USRDFN.
        let p = TlsPolicy::from_sysvals(&["*OPSYS"], &["*AES_256_GCM_SHA384"], "*OPSYS");
        assert_eq!(p.suites.len(), 9);
        let none = TlsPolicy::from_sysvals(&["*SSLV3"], &[], "*OPSYS");
        assert!(none.versions().is_err());
        let p = TlsPolicy::from_sysvals(&["*TLSV1.2"], &["*AES_128_GCM_SHA256"], "*USRDFN");
        assert!(p.provider().is_err(), "a 1.3 suite with only 1.2 enabled");
    }
}
