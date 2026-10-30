// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*CTLD`: a controller description. Phase 8 has the virtual
//! workstation controller `QPACTL01` (`*VWS`) the TN5250E displays
//! hang off; Phase 10 adds the network controllers (`*NET`) a vary on
//! of an Ethernet line creates, each attached to its line. The line
//! field sits in what was reserved, so version-1 payloads read as
//! attached to none.

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
        field("Auto", 11, 1),
        field("Type", 12, 10),
        field("Text", 22, 50),
        field("Line", 72, 10),
        field("Reserved2", 82, 14),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMCTLD";
const VERSION: u64 = 1;

/// A controller description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ctld {
    /// Varied on at IPL.
    pub online: bool,
    /// `*VWS` for the virtual workstation controller, `*NET` for a
    /// network controller.
    pub ctltype: String,
    /// `TEXT`.
    pub text: String,
    /// The attached line, blank for none.
    pub line: String,
    /// Created by automatic configuration (deleted with its line).
    pub auto: bool,
}

impl Ctld {
    /// `QPACTL01`.
    #[must_use]
    pub fn virtual_workstation() -> Self {
        Self {
            online: true,
            ctltype: "*VWS".into(),
            text: "Virtual workstation controller".into(),
            line: String::new(),
            auto: false,
        }
    }

    /// The network controller automatic configuration makes for `line`.
    #[must_use]
    pub fn network(line: &str) -> Self {
        Self {
            online: false,
            ctltype: "*NET".into(),
            text: "Created by automatic configuration".into(),
            line: line.to_string(),
            auto: true,
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Online", self.online);
        r.set_flag("Auto", self.auto);
        r.set_text("Type", &self.ctltype);
        r.set_text("Text", &self.text);
        r.set_text("Line", &self.line);
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
            line: r.text("Line"),
            auto: r.flag("Auto"),
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
        let n = Ctld::network("ETHLINE");
        assert_eq!(Ctld::decode(&n.encode()).unwrap(), n);
    }

    #[test]
    fn phase8_payloads_attach_to_no_line() {
        // A Phase 8 payload: zeros where the line now sits.
        let mut b = Ctld::virtual_workstation().encode();
        b[11] = 0;
        b[72..82].fill(0);
        let c = Ctld::decode(&b).unwrap();
        assert_eq!(c.line, "");
        assert!(!c.auto);
    }
}
