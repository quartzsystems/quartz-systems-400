// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The HTTP administration instance's ports: 2001 plain and 2005 with
//! TLS, as IBM's `*ADMIN` instance listens. No command changes them yet
//! (IBM's are in the instance's configuration file, which comes with
//! the full HTTP server); the bag's `PORT` and `TLSPORT` move them, and
//! `TLSPORT(0)` leaves TLS off.

use super::{get, keyword};

/// The plain port.
pub const PORT: u16 = 2001;
/// The TLS port.
pub const TLS_PORT: u16 = 2005;

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
        assert_eq!((d.port, d.tls_port), (2001, 2005));
        let moved = HttpAttrs::from_bag(&[("PORT".into(), "8001".into()), ("tlsport".into(), "0".into())]);
        assert_eq!(moved.rqsdta(), "PORT(8001) TLSPORT(0)");
        assert_eq!(HttpAttrs::from_rqsdta(&moved.rqsdta()), moved);
        assert_eq!(HttpAttrs::from_rqsdta(""), d);
    }
}
