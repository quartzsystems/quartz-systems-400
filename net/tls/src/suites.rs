// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cipher suites: IBM's `QSSLCSL` names, the IANA codes programs give
//! (`SSL_Init`) and the IANA names (`GSK_*_CIPHER_SPECS_EX`) and the rustls suites behind
//! them. Suites rustls does not implement (CBC, RC4, 3DES, NULL, export)
//! are listed so names and codes are recognized and reported, never
//! negotiated; docs/CERTS.md has the table.

use rustls::SupportedCipherSuite;

/// One suite IBM i knows.
#[derive(Clone, Copy, Debug)]
pub struct IbmSuite {
    /// The `QSSLCSL` name.
    pub name: &'static str,
    /// The IANA code (the `TLS_*` value of `qsossl.h`).
    pub code: u16,
    /// A TLS 1.3 suite.
    pub tls13: bool,
    /// The rustls suite, when there is one.
    pub rustls: Option<&'static SupportedCipherSuite>,
}

impl IbmSuite {
    /// Whether the engine can negotiate it.
    pub fn supported(&self) -> bool {
        self.rustls.is_some()
    }

    /// The IANA name (`TLS_AES_128_GCM_SHA256`).
    pub fn iana_name(&self) -> String {
        let body = &self.name[1..];
        if self.tls13 {
            return format!("TLS_{body}");
        }
        if let Some(rest) = body.strip_prefix("RSA_EXPORT_") {
            return format!("TLS_RSA_EXPORT_WITH_{rest}");
        }
        for kw in ["_AES", "_CHACHA", "_3DES", "_RC4", "_NULL", "_DES"] {
            if let Some(at) = body.find(kw) {
                return format!("TLS_{}_WITH{}", &body[..at], &body[at..]);
            }
        }
        format!("TLS_{body}")
    }
}

use rustls_graviola::suites as g;

/// Every suite, in IBM's `*OPSYS` preference order; the unsupported
/// ones follow in the order IBM lists them.
#[rustfmt::skip]
pub const ALL: &[IbmSuite] = &[
    IbmSuite { name: "*AES_128_GCM_SHA256", code: 0x1301, tls13: true, rustls: Some(&g::TLS13_AES_128_GCM_SHA256) },
    IbmSuite { name: "*AES_256_GCM_SHA384", code: 0x1302, tls13: true, rustls: Some(&g::TLS13_AES_256_GCM_SHA384) },
    IbmSuite { name: "*CHACHA20_POLY1305_SHA256", code: 0x1303, tls13: true, rustls: Some(&g::TLS13_CHACHA20_POLY1305_SHA256) },
    IbmSuite { name: "*ECDHE_ECDSA_AES_128_GCM_SHA256", code: 0xC02B, tls13: false, rustls: Some(&g::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256) },
    IbmSuite { name: "*ECDHE_ECDSA_AES_256_GCM_SHA384", code: 0xC02C, tls13: false, rustls: Some(&g::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384) },
    IbmSuite { name: "*ECDHE_RSA_AES_128_GCM_SHA256", code: 0xC02F, tls13: false, rustls: Some(&g::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256) },
    IbmSuite { name: "*ECDHE_RSA_AES_256_GCM_SHA384", code: 0xC030, tls13: false, rustls: Some(&g::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384) },
    IbmSuite { name: "*ECDHE_ECDSA_CHACHA20_POLY1305_SHA256", code: 0xCCA9, tls13: false, rustls: Some(&g::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256) },
    IbmSuite { name: "*ECDHE_RSA_CHACHA20_POLY1305_SHA256", code: 0xCCA8, tls13: false, rustls: Some(&g::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256) },
    // Known, not negotiated.
    IbmSuite { name: "*RSA_AES_128_GCM_SHA256", code: 0x009C, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_AES_256_GCM_SHA384", code: 0x009D, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_ECDSA_AES_128_CBC_SHA256", code: 0xC023, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_ECDSA_AES_256_CBC_SHA384", code: 0xC024, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_RSA_AES_128_CBC_SHA256", code: 0xC027, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_RSA_AES_256_CBC_SHA384", code: 0xC028, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_AES_128_CBC_SHA256", code: 0x003C, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_AES_128_CBC_SHA", code: 0x002F, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_AES_256_CBC_SHA256", code: 0x003D, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_AES_256_CBC_SHA", code: 0x0035, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_ECDSA_3DES_EDE_CBC_SHA", code: 0xC008, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_RSA_3DES_EDE_CBC_SHA", code: 0xC012, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_3DES_EDE_CBC_SHA", code: 0x000A, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_ECDSA_RC4_128_SHA", code: 0xC007, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_RSA_RC4_128_SHA", code: 0xC011, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_RC4_128_SHA", code: 0x0005, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_RC4_128_MD5", code: 0x0004, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_NULL_SHA256", code: 0x003B, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_NULL_SHA", code: 0x0002, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_NULL_MD5", code: 0x0001, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_ECDSA_NULL_SHA", code: 0xC006, tls13: false, rustls: None },
    IbmSuite { name: "*ECDHE_RSA_NULL_SHA", code: 0xC010, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_DES_CBC_SHA", code: 0x0009, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_EXPORT_RC4_40_MD5", code: 0x0003, tls13: false, rustls: None },
    IbmSuite { name: "*RSA_EXPORT_RC2_CBC_40_MD5", code: 0x0006, tls13: false, rustls: None },
];

/// The suite with this `QSSLCSL` name (case-insensitive, `*` optional).
pub fn by_name(name: &str) -> Option<&'static IbmSuite> {
    let n = name.trim().trim_start_matches('*').to_ascii_uppercase();
    ALL.iter().find(|s| s.name[1..] == n)
}

/// The suite with this IANA code.
pub fn by_code(code: u16) -> Option<&'static IbmSuite> {
    ALL.iter().find(|s| s.code == code)
}

/// The suite behind a negotiated rustls suite.
pub fn of_rustls(suite: SupportedCipherSuite) -> Option<&'static IbmSuite> {
    let code = u16::from(suite.suite());
    by_code(code)
}

/// The `*OPSYS` list: every supported suite in preference order.
pub fn opsys() -> Vec<&'static IbmSuite> {
    ALL.iter().filter(|s| s.supported()).collect()
}

/// The suite with this IANA name (`TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256`).
pub fn by_iana_name(name: &str) -> Option<&'static IbmSuite> {
    let n = name.trim().to_ascii_uppercase();
    ALL.iter().find(|s| s.iana_name() == n)
}

/// Parse a `GSK_*_CIPHER_SPECS_EX` string: IANA suite names separated
/// by commas. Four hex digits (`C02F`) are taken as a code too. Suites
/// unknown or not negotiable come back in the second list.
pub fn parse_spec_string(spec: &str) -> (Vec<&'static IbmSuite>, Vec<String>) {
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let found = by_iana_name(item).or_else(|| {
            (item.len() == 4)
                .then(|| u16::from_str_radix(item, 16).ok())
                .flatten()
                .and_then(by_code)
        });
        match found {
            Some(x) if x.supported() => ok.push(x),
            _ => bad.push(item.to_string()),
        }
    }
    (ok, bad)
}

/// Parse a two-digit spec string (`GSK_V3_CIPHER_SPECS`,
/// `GSK_TLSV12_CIPHER_SPECS`, `"2F3C"`): each pair is the low byte of a
/// `0x00nn` code.
pub fn parse_short_specs(spec: &str) -> Vec<u16> {
    spec.trim()
        .as_bytes()
        .chunks(2)
        .filter_map(|c| u16::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_codes_and_iana() {
        assert_eq!(
            by_name("*ECDHE_RSA_AES_128_GCM_SHA256").unwrap().code,
            0xC02F
        );
        assert_eq!(by_name("aes_128_gcm_sha256").unwrap().code, 0x1301);
        assert!(by_name("*RSA_RC4_128_SHA").is_some_and(|s| !s.supported()));
        assert_eq!(by_code(0x1303).unwrap().name, "*CHACHA20_POLY1305_SHA256");
        assert_eq!(
            by_code(0xC02B).unwrap().iana_name(),
            "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256"
        );
        assert_eq!(
            by_code(0x1302).unwrap().iana_name(),
            "TLS_AES_256_GCM_SHA384"
        );
        assert_eq!(
            by_code(0x0003).unwrap().iana_name(),
            "TLS_RSA_EXPORT_WITH_RC4_40_MD5"
        );
        assert_eq!(opsys().len(), 9);
        let (ok, bad) = parse_spec_string(
            "TLS_AES_128_GCM_SHA256, TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,TLS_RSA_WITH_RC4_128_SHA,C02B,NOPE",
        );
        assert_eq!(
            ok.iter().map(|s| s.code).collect::<Vec<_>>(),
            vec![0x1301, 0xC02F, 0xC02B]
        );
        assert_eq!(bad, vec!["TLS_RSA_WITH_RC4_128_SHA", "NOPE"]);
        assert_eq!(parse_short_specs("2F3C"), vec![0x002F, 0x003C]);
        for s in ALL {
            assert_eq!(
                by_iana_name(&s.iana_name()).unwrap().code,
                s.code,
                "{}",
                s.name
            );
        }
        let mut codes: Vec<u16> = ALL.iter().map(|s| s.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), ALL.len());
    }
}
