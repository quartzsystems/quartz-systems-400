// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A response file over the network (docs/INSTALL.md, "Network install"):
//! `http://`, `https://` ([`http`]) and `tftp://` ([`tftp`]) URLs, and
//! the file beside a network boot's boot file ([`beside`]): the DHCP
//! server that handed the firmware the UKI names where the answers are.

pub mod http;
pub mod tftp;

use std::net::Ipv4Addr;

/// The file a network boot looks for beside its boot file.
pub const BESIDE: &str = "INSTALL.RSP";
/// The most bytes a response file may have.
pub const MAX_BYTES: usize = 1 << 20;

/// A URL taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// `http`, `https` or `tftp`.
    pub scheme: String,
    /// Host name or address.
    pub host: String,
    /// Port (80, 443, 69 by default).
    pub port: u16,
    /// Path with query, starting with `/`.
    pub path: String,
}

impl Url {
    /// Parse `scheme://host[:port]/path`.
    pub fn parse(u: &str) -> Result<Self, String> {
        let (scheme, rest) = u
            .split_once("://")
            .ok_or_else(|| format!("{u}: not a URL"))?;
        let scheme = scheme.to_ascii_lowercase();
        let default = match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            "tftp" => 69,
            _ => return Err(format!("{u}: only http, https and tftp are fetched")),
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (
                h,
                p.parse()
                    .map_err(|_| format!("{u}: port '{p}' is not a number"))?,
            ),
            _ => (authority, default),
        };
        let host = host.trim_matches(['[', ']']).to_string();
        if host.is_empty() {
            return Err(format!("{u}: no host"));
        }
        Ok(Self {
            scheme,
            host,
            port,
            path: path.to_string(),
        })
    }

    /// Another path on the same server (a redirect's relative location).
    pub fn join(&self, location: &str) -> Result<Self, String> {
        if location.contains("://") {
            return Self::parse(location);
        }
        let path = if location.starts_with('/') {
            location.to_string()
        } else {
            let dir = self.path.rsplit_once('/').map_or("", |(d, _)| d);
            format!("{dir}/{location}")
        };
        Ok(Self {
            path,
            ..self.clone()
        })
    }
}

/// The response file beside a boot file a DHCP server named: a URL boot
/// file (UEFI HTTP boot) gives the same directory on that server; a
/// plain one (PXE over TFTP) the same directory on the next server.
pub fn beside(next_server: Ipv4Addr, boot_file: &str) -> String {
    let f = boot_file.trim();
    if f.contains("://") {
        let dir = f.rsplit_once('/').map_or(f, |(d, _)| d);
        return format!("{dir}/{BESIDE}");
    }
    let f = f.trim_start_matches('/');
    match f.rsplit_once('/') {
        Some((dir, _)) => format!("tftp://{next_server}/{dir}/{BESIDE}"),
        None => format!("tftp://{next_server}/{BESIDE}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = Url::parse("HTTP://10.0.2.2:8080/qs400/install.rsp?x=1").unwrap();
        assert_eq!(
            (u.scheme.as_str(), u.host.as_str(), u.port, u.path.as_str()),
            ("http", "10.0.2.2", 8080, "/qs400/install.rsp?x=1")
        );
        let u = Url::parse("https://install.example.com").unwrap();
        assert_eq!((u.port, u.path.as_str()), (443, "/"));
        assert_eq!(
            Url::parse("tftp://[fd00::1]:6969/a").unwrap().host,
            "fd00::1"
        );
        assert!(Url::parse("ftp://x/y").is_err());
        assert!(Url::parse("http:///y").is_err());
        assert_eq!(u.join("/other.rsp").unwrap().path, "/other.rsp");
        let v = Url::parse("http://h/a/b.rsp").unwrap();
        assert_eq!(v.join("c.rsp").unwrap().path, "/a/c.rsp");
        assert_eq!(v.join("https://k/z").unwrap().host, "k");
    }

    #[test]
    fn beside_the_boot_file() {
        let s = Ipv4Addr::new(10, 0, 2, 2);
        assert_eq!(
            beside(s, "qs400/BOOTX64.EFI"),
            "tftp://10.0.2.2/qs400/INSTALL.RSP"
        );
        assert_eq!(beside(s, "/BOOTX64.EFI"), "tftp://10.0.2.2/INSTALL.RSP");
        assert_eq!(
            beside(s, "http://boot.example.com/qs400/BOOTX64.EFI"),
            "http://boot.example.com/qs400/INSTALL.RSP"
        );
    }
}
