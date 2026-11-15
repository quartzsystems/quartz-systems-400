// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hello each side says first, once TLS is up.

use ipc::wire_struct;

use crate::open_enum;

open_enum! {
    /// What a side is.
    PeerKind {
        /// A management console.
        Console = 1,
        /// A virtual server host.
        Host = 2,
        /// A standalone system.
        Standalone = 3,
    }
}

/// The longest name in a hello.
pub const MAX_NAME: usize = 256;
/// The longest release in a hello.
pub const MAX_RELEASE: usize = 32;
/// The longest serial number in a hello.
pub const MAX_SERIAL: usize = 64;

/// A side's hello.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    /// The protocol version it speaks, at most.
    pub version: u16,
    /// Its capabilities ([`crate::caps`]).
    pub caps: u64,
    /// What it is.
    pub kind: PeerKind,
    /// Its name: a console's, a host's, a system's.
    pub name: String,
    /// Its release, in its own form (`V1R2M0`).
    pub release: String,
    /// Its serial number; empty when it has none (a console may not).
    pub serial: String,
}
wire_struct!(Hello {
    version,
    caps,
    kind,
    name,
    release,
    serial
});

/// What two hellos agree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Agreed {
    /// The lower of the two versions.
    pub version: u16,
    /// The capabilities both have.
    pub caps: u64,
}

impl Agreed {
    /// Whether both have the family whose capability is `cap`.
    #[must_use]
    pub fn has(&self, cap: u64) -> bool {
        cap != 0 && self.caps & cap == cap
    }
}

impl Hello {
    /// What this hello and the peer's agree on.
    #[must_use]
    pub fn agree(&self, theirs: &Hello) -> Agreed {
        Agreed {
            version: self.version.min(theirs.version),
            caps: self.caps & theirs.caps,
        }
    }

    /// Whether the hello is within its bounds: a version, and fields no
    /// longer than their limits.
    ///
    /// # Errors
    /// What is out of bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        if self.version == 0 {
            return Err("version 0");
        }
        if self.name.len() > MAX_NAME {
            return Err("a name longer than 256 bytes");
        }
        if self.release.len() > MAX_RELEASE {
            return Err("a release longer than 32 bytes");
        }
        if self.serial.len() > MAX_SERIAL {
            return Err("a serial number longer than 64 bytes");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps;

    #[test]
    fn the_older_version_and_the_common_capabilities() {
        let host = Hello {
            version: 2,
            caps: caps::HOST_V1 | 1 << 40,
            kind: PeerKind::Host,
            name: "VS01".into(),
            release: "V1R1M0".into(),
            serial: "M525400123456".into(),
        };
        let console = Hello {
            version: 1,
            caps: caps::MANAGED | caps::PARTITIONS,
            kind: PeerKind::Console,
            name: "QSMC01".into(),
            release: "V1R1M0".into(),
            serial: String::new(),
        };
        let a = host.agree(&console);
        assert_eq!(a, console.agree(&host));
        assert_eq!(a.version, 1);
        assert!(a.has(caps::PARTITIONS) && a.has(caps::BASE));
        assert!(!a.has(caps::VRES) && !a.has(0));
        // Read back, with a later version's field after it.
        let mut bytes = ipc::encode(&host);
        bytes.extend_from_slice(b"later");
        let back: Hello = crate::message::body(0, 0, &bytes).unwrap();
        assert_eq!(back, host);
        assert!(host.check().is_ok());
        let mut long = console.clone();
        long.name = "n".repeat(MAX_NAME + 1);
        assert!(long.check().is_err());
        // A kind a later version adds is kept.
        let mut odd = ipc::encode(&console);
        odd[10] = 9;
        let back: Hello = crate::message::body(0, 0, &odd).unwrap();
        assert_eq!(back.kind, PeerKind::Other(9));
    }
}
