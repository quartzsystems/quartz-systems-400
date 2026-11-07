// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The HTTP administration instance's ports: 2001 plain and 2010 with
//! TLS (`as-admin-http` and `as-admin-https`). No command changes them
//! (IBM's are in the instance's configuration file, which comes with
//! the full HTTP server); the bag's `PORT` and `TLSPORT` move them, and
//! `TLSPORT(0)` leaves TLS off. Navigator, started with it, is on 2002
//! and 2005 (`NAVPORT`, `NAVTLSPORT`), Access for Web on 2003 and 2006
//! (`AFWPORT`, `AFWTLSPORT`).

use super::{get, keyword};

/// The plain port.
pub const PORT: u16 = 2001;
/// The TLS port.
pub const TLS_PORT: u16 = 2010;
/// Navigator's plain port.
pub const NAV_PORT: u16 = 2002;
/// Navigator's TLS port.
pub const NAV_TLS_PORT: u16 = 2005;
/// Access for Web's plain port.
pub const AFW_PORT: u16 = 2003;
/// Access for Web's TLS port.
pub const AFW_TLS_PORT: u16 = 2006;

/// The administration instance's ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpAttrs {
    /// Plain HTTP.
    pub port: u16,
    /// HTTPS; 0 for none.
    pub tls_port: u16,
}

impl Default for HttpAttrs {
    fn default() -> Self {
        Self {
            port: PORT,
            tls_port: TLS_PORT,
        }
    }
}

impl HttpAttrs {
    /// From the server table's bag.
    pub fn from_bag(bag: &[(String, String)]) -> Self {
        let d = Self::default();
        Self {
            port: get(bag, "PORT").and_then(|p| p.trim().parse().ok()).unwrap_or(d.port),
            tls_port: get(bag, "TLSPORT")
                .and_then(|p| p.trim().parse().ok())
                .unwrap_or(d.tls_port),
        }
    }

    /// Navigator's ports (`NAVIGATOR`, 2002 and 2005): the bag's
    /// `NAVPORT` and `NAVTLSPORT`; no TLS when the administration
    /// instance has none.
    pub fn navigator(bag: &[(String, String)]) -> Self {
        let admin = Self::from_bag(bag);
        let port = |k, d| get(bag, k).and_then(|p| p.trim().parse().ok()).unwrap_or(d);
        Self {
            port: port("NAVPORT", NAV_PORT),
            tls_port: if admin.tls_port == 0 { 0 } else { port("NAVTLSPORT", NAV_TLS_PORT) },
        }
    }

    /// Access for Web's ports (`ACCESSWEB`, 2003 and 2006): the bag's
    /// `AFWPORT` and `AFWTLSPORT`; no TLS when the administration
    /// instance has none.
    pub fn accessweb(bag: &[(String, String)]) -> Self {
        let admin = Self::from_bag(bag);
        let port = |k, d| get(bag, k).and_then(|p| p.trim().parse().ok()).unwrap_or(d);
        Self {
            port: port("AFWPORT", AFW_PORT),
            tls_port: if admin.tls_port == 0 { 0 } else { port("AFWTLSPORT", AFW_TLS_PORT) },
        }
    }

    /// The listener job's request data.
    pub fn rqsdta(&self) -> String {
        format!("PORT({}) TLSPORT({})", self.port, self.tls_port)
    }

    /// From the listener job's request data.
    pub fn from_rqsdta(text: &str) -> Self {
        let d = Self::default();
        Self {
            port: keyword(text, "PORT").and_then(|p| p.parse().ok()).unwrap_or(d.port),
            tls_port: keyword(text, "TLSPORT")
                .and_then(|p| p.parse().ok())
                .unwrap_or(d.tls_port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports() {
        let d = HttpAttrs::from_bag(&[]);
        assert_eq!((d.port, d.tls_port), (2001, 2010));
        let moved = HttpAttrs::from_bag(&[("PORT".into(), "8001".into()), ("tlsport".into(), "0".into())]);
        assert_eq!(moved.rqsdta(), "PORT(8001) TLSPORT(0)");
        assert_eq!(HttpAttrs::from_rqsdta(&moved.rqsdta()), moved);
        assert_eq!(HttpAttrs::from_rqsdta(""), d);
        assert_eq!(HttpAttrs::navigator(&[]).rqsdta(), "PORT(2002) TLSPORT(2005)");
        assert_eq!(HttpAttrs::navigator(&[("TLSPORT".into(), "0".into())]).tls_port, 0);
        assert_eq!(HttpAttrs::accessweb(&[]).rqsdta(), "PORT(2003) TLSPORT(2006)");
        assert_eq!(HttpAttrs::accessweb(&[("AFWPORT".into(), "8003".into())]).port, 8003);
    }
}
