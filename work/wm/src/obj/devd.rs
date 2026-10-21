// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DEVD`: a device description. Phase 5 has one printer device, the
//! capture device `QSCAPTURE` of the Quartz type `QSCP`: its writer
//! renders spooled files to text and PDF kept in the store instead of
//! sending them to a printer. `MIGRATE(phase10)`: real printers,
//! `CRTDEVPRT`, varying on and off.

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "DeviceDescription",
    size: Some(128),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Online", 10, 1),
        field("Reserved", 11, 1),
        field("DevCls", 12, 10),
        field("Type", 22, 10),
        field("Model", 32, 10),
        field("OutqName", 42, 10),
        field("OutqLib", 52, 10),
        field("Text", 62, 50),
        field("Reserved2", 112, 16),
    ],
};

const MAGIC: &[u8; 8] = b"QSWMDEVD";
const VERSION: u64 = 1;

/// The capture device's name.
pub const CAPTURE: &str = "QSCAPTURE";

/// A device description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Devd {
    /// Varied on.
    pub online: bool,
    /// The class: `*PRT`.
    pub devcls: String,
    /// The type: `QSCP` for the capture device.
    pub devtype: String,
    /// The model.
    pub model: String,
    /// The device's output queue, `lib/name`.
    pub outq: String,
    /// `TEXT`.
    pub text: String,
}

impl Devd {
    /// The capture device.
    #[must_use]
    pub fn capture() -> Self {
        Self {
            online: true,
            devcls: "*PRT".into(),
            devtype: "QSCP".into(),
            model: "1".into(),
            outq: format!("QUSRSYS/{CAPTURE}"),
            text: "Capture printer: text and PDF in the store".into(),
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Online", self.online);
        r.set_text("DevCls", &self.devcls);
        r.set_text("Type", &self.devtype);
        r.set_text("Model", &self.model);
        let (l, n) = self.outq.split_once('/').unwrap_or(("", &self.outq));
        r.set_text("OutqName", n);
        r.set_text("OutqLib", l);
        r.set_text("Text", &self.text);
        r.buf
    }

    /// Read a payload back.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(Error::Corrupt("not a device description".into()));
        }
        Ok(Self {
            online: r.flag("Online"),
            devcls: r.text("DevCls"),
            devtype: r.text("Type"),
            model: r.text("Model"),
            outq: format!("{}/{}", r.text("OutqLib"), r.text("OutqName")),
            text: r.text("Text"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let d = Devd::capture();
        assert_eq!(Devd::decode(&d.encode()).unwrap(), d);
    }
}
