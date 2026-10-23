// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*CTLD`: a controller description. Phase 8 has one, the virtual
//! workstation controller `QPACTL01` (`*VWS`) the TN5250E displays
//! hang off. `MIGRATE(phase10)`: lines and real controllers.

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "ControllerDescription",
    size: Some(96),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Online", 10, 1),
        field("Reserved", 11, 1),
        field("Type", 12, 10),
        field("Text", 22, 50),
        field("Reserved2", 72, 24),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMCTLD";
const VERSION: u64 = 1;

/// A controller description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ctld {
    /// Varied on at IPL.
    pub online: bool,
    /// `*VWS` for the virtual workstation controller.
    pub ctltype: String,
    /// `TEXT`.
    pub text: String,
}

impl Ctld {
    /// `QPACTL01`.
    #[must_use]
    pub fn virtual_workstation() -> Self {
        Self {
            online: true,
            ctltype: "*VWS".into(),
            text: "Virtual workstation controller".into(),
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Online", self.online);
        r.set_text("Type", &self.ctltype);
        r.set_text("Text", &self.text);
        r.buf
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a controller description".into()));
        }
        Ok(Self {
            online: r.flag("Online"),
            ctltype: r.text("Type"),
            text: r.text("Text"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let c = Ctld::virtual_workstation();
        assert_eq!(Ctld::decode(&c.encode()).unwrap(), c);
    }
}
