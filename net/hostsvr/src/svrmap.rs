// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The port mapper (`as-svrmap`, port 449). Not framed: the client
//! sends a service name in ASCII with no terminator; the server answers
//! five bytes, `+` (0x2B) and the port as a big-endian u32. Any other
//! first byte means the service is unknown.

use crate::Error;

/// The success marker.
pub const FOUND: u8 = b'+';

/// A lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The service name (`as-signon`, `as-database-s`, ...).
    pub service: String,
}

impl Request {
    /// Decode the request bytes.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.is_empty() || !b.iter().all(u8::is_ascii_graphic) {
            return Err(Error::Invalid(
                "port mapper request is not an ASCII name".into(),
            ));
        }
        Ok(Self {
            service: String::from_utf8_lossy(b).into_owned(),
        })
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        self.service.as_bytes().to_vec()
    }
}

/// The answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The service's port.
    Port(u32),
    /// Anything else: the service is unknown.
    Other(Vec<u8>),
}

impl Reply {
    /// Decode the reply bytes.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        match b {
            [FOUND, p0, p1, p2, p3] => Ok(Self::Port(u32::from_be_bytes([*p0, *p1, *p2, *p3]))),
            [] => Err(Error::Invalid("empty port mapper reply".into())),
            other => Ok(Self::Other(other.to_vec())),
        }
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Port(p) => {
                let mut v = vec![FOUND];
                v.extend_from_slice(&p.to_be_bytes());
                v
            }
            Self::Other(b) => b.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let r = Request::decode(b"as-signon").unwrap();
        assert_eq!(r.service, "as-signon");
        assert_eq!(r.encode(), b"as-signon");
        let p = Reply::decode(&[0x2b, 0, 0, 0x21, 0x1c]).unwrap();
        assert_eq!(p, Reply::Port(8476));
        assert_eq!(p.encode(), vec![0x2b, 0, 0, 0x21, 0x1c]);
    }
}
