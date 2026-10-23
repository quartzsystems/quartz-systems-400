// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*DEVD`: a device description. Phase 5 has one printer device, the
//! capture device `QSCAPTURE` of the Quartz type `QSCP`, whose writer
//! renders spooled files to text and PDF kept in the store. Phase 8
//! adds the virtual displays of TN5250E sessions (`QPADEVnnnn` on the
//! controller `QPACTL01`) and the console `QSCONSOLE`.
//! `MIGRATE(phase10)`: real printers, `CRTDEVPRT`.

use crate::rec::{Layout, Rec, field};
use crate::{Error, Result};

/// The payload.
pub const LAYOUT: Layout = Layout {
    name: "DeviceDescription",
    size: Some(192),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Online", 10, 1),
        field("Status", 11, 1),
        field("DevCls", 12, 10),
        field("Type", 22, 10),
        field("Model", 32, 10),
        field("OutqName", 42, 10),
        field("OutqLib", 52, 10),
        field("Text", 62, 50),
        field("Ctl", 112, 10),
        field("KbdType", 122, 10),
        field("MsgqName", 132, 10),
        field("MsgqLib", 142, 10),
        field("Virtual", 152, 1),
        field("AlwBln", 153, 1),
        field("Reserved2", 154, 38),
    ],
};

/// The version-1 payload of Phase 5, still read.
const LAYOUT_V1: Layout = Layout {
    name: "DeviceDescriptionV1",
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
const VERSION: u64 = 2;

/// The console display's name.
pub const CONSOLE: &str = "QSCONSOLE";

/// The capture device's name.
pub const CAPTURE: &str = "QSCAPTURE";

/// The virtual workstation controller.
pub const VIRTUAL_CONTROLLER: &str = "QPACTL01";

/// The prefix of automatically created virtual displays.
pub const VIRTUAL_PREFIX: &str = "QPADEV";

/// What a display device is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum DevStatus {
    /// Varied off.
    #[default]
    VariedOff = 0,
    /// Varied on, no session.
    VariedOn = 1,
    /// A session shows the sign-on display.
    SignonDisplay = 2,
    /// A job is signed on.
    Active = 3,
    /// The session dropped; recovery pending.
    RecoveryPending = 4,
}

impl DevStatus {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::VariedOn,
            2 => Self::SignonDisplay,
            3 => Self::Active,
            4 => Self::RecoveryPending,
            _ => Self::VariedOff,
        }
    }

    /// As `WRKCFGSTS` shows it.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::VariedOff => "VARIED OFF",
            Self::VariedOn => "VARIED ON",
            Self::SignonDisplay => "SIGNON DISPLAY",
            Self::Active => "ACTIVE",
            Self::RecoveryPending => "RCYPND",
        }
    }
}

/// A device description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Devd {
    /// Varied on (`ONLINE(*YES)` and varied on at IPL).
    pub online: bool,
    /// What it is doing now.
    pub status: DevStatus,
    /// The class: `*PRT`, `*DSP`, `*VRT`.
    pub devcls: String,
    /// The type: `QSCP` for the capture device, `3179`, `3477`, `5555`.
    pub devtype: String,
    /// The model: `2`, `FC`, `C01`.
    pub model: String,
    /// The device's output queue, `lib/name`.
    pub outq: String,
    /// `TEXT`.
    pub text: String,
    /// The controller: `QPACTL01` for virtual displays.
    pub ctl: String,
    /// `KBDTYPE`.
    pub kbdtype: String,
    /// The message queue, `lib/name`; empty for `*CTLD`.
    pub msgq: String,
    /// Created automatically (`QAUTOVRT`).
    pub virtual_: bool,
    /// `ALWBLN(*YES)`.
    pub alwbln: bool,
}

impl Devd {
    /// The capture device.
    #[must_use]
    pub fn capture() -> Self {
        Self {
            online: true,
            status: DevStatus::VariedOn,
            devcls: "*PRT".into(),
            devtype: "QSCP".into(),
            model: "1".into(),
            outq: format!("QUSRSYS/{CAPTURE}"),
            text: "Capture printer: text and PDF in the store".into(),
            ctl: String::new(),
            kbdtype: String::new(),
            msgq: String::new(),
            virtual_: false,
            alwbln: true,
        }
    }

    /// The console display.
    #[must_use]
    pub fn console() -> Self {
        Self {
            online: true,
            status: DevStatus::VariedOn,
            devcls: "*DSP".into(),
            devtype: "3477".into(),
            model: "FC".into(),
            outq: String::new(),
            text: "System console display".into(),
            ctl: VIRTUAL_CONTROLLER.into(),
            kbdtype: "USB".into(),
            msgq: "QSYS/QSYSOPR".into(),
            virtual_: false,
            alwbln: true,
        }
    }

    /// A virtual display a TN5250E session asked for.
    #[must_use]
    pub fn virtual_display(devtype: &str, model: &str, kbdtype: &str, auto: bool) -> Self {
        Self {
            online: true,
            status: DevStatus::VariedOn,
            devcls: "*VRT".into(),
            devtype: devtype.into(),
            model: model.into(),
            outq: String::new(),
            text: if auto {
                "Virtual display created by QAUTOVRT".into()
            } else {
                String::new()
            },
            ctl: VIRTUAL_CONTROLLER.into(),
            kbdtype: kbdtype.into(),
            msgq: "QSYS/QSYSOPR".into(),
            virtual_: auto,
            alwbln: true,
        }
    }

    /// Whether this is a display.
    #[must_use]
    pub fn is_display(&self) -> bool {
        matches!(self.devcls.as_str(), "*DSP" | "*VRT") && !is_printer_type(&self.devtype)
    }

    /// Whether this is a printer (the capture device or a TN5250E
    /// printer session's device).
    #[must_use]
    pub fn is_printer(&self) -> bool {
        self.devcls == "*PRT" || is_printer_type(&self.devtype)
    }

    /// A virtual printer a TN5250E printer session asked for; its
    /// output queue is `QUSRSYS/<name>`.
    #[must_use]
    pub fn virtual_printer(name: &str, devtype: &str, model: &str) -> Self {
        Self {
            online: true,
            status: DevStatus::VariedOn,
            devcls: "*VRT".into(),
            devtype: devtype.into(),
            model: model.into(),
            outq: format!("QUSRSYS/{name}"),
            text: "Virtual printer of a TN5250E printer session".into(),
            ctl: VIRTUAL_CONTROLLER.into(),
            kbdtype: String::new(),
            msgq: "QSYS/QSYSOPR".into(),
            virtual_: false,
            alwbln: false,
        }
    }

    /// The payload.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_flag("Online", self.online);
        r.set_uint("Status", u64::from(self.status as u8));
        r.set_text("DevCls", &self.devcls);
        r.set_text("Type", &self.devtype);
        r.set_text("Model", &self.model);
        let (l, n) = self.outq.split_once('/').unwrap_or(("", &self.outq));
        r.set_text("OutqName", n);
        r.set_text("OutqLib", l);
        r.set_text("Text", &self.text);
        r.set_text("Ctl", &self.ctl);
        r.set_text("KbdType", &self.kbdtype);
        let (l, n) = self.msgq.split_once('/').unwrap_or(("", &self.msgq));
        r.set_text("MsgqName", n);
        r.set_text("MsgqLib", l);
        r.set_flag("Virtual", self.virtual_);
        r.set_flag("AlwBln", self.alwbln);
        r.buf
    }

    /// Read a payload back, of either version.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let v1 = b.len() == LAYOUT_V1.size.unwrap_or(0);
        let r = Rec::read(if v1 { &LAYOUT_V1 } else { &LAYOUT }, b)?;
        if r.bytes("Magic") != MAGIC || (r.uint("Version") != VERSION && !v1) {
            return Err(Error::Corrupt("not a device description".into()));
        }
        let outq = |lib: String, name: String| {
            if name.is_empty() {
                String::new()
            } else {
                format!("{lib}/{name}")
            }
        };
        let mut d = Self {
            online: r.flag("Online"),
            status: DevStatus::VariedOff,
            devcls: r.text("DevCls"),
            devtype: r.text("Type"),
            model: r.text("Model"),
            outq: outq(r.text("OutqLib"), r.text("OutqName")),
            text: r.text("Text"),
            ctl: String::new(),
            kbdtype: String::new(),
            msgq: String::new(),
            virtual_: false,
            alwbln: true,
        };
        if v1 {
            d.status = if d.online {
                DevStatus::VariedOn
            } else {
                DevStatus::VariedOff
            };
        } else {
            d.status = DevStatus::from_byte(u8::try_from(r.uint("Status")).unwrap_or(0));
            d.ctl = r.text("Ctl");
            d.kbdtype = r.text("KbdType");
            d.msgq = outq(r.text("MsgqLib"), r.text("MsgqName"));
            d.virtual_ = r.flag("Virtual");
            d.alwbln = r.flag("AlwBln");
        }
        Ok(d)
    }
}

/// Whether a device type is a printer's.
#[must_use]
pub fn is_printer_type(devtype: &str) -> bool {
    matches!(
        devtype,
        "QSCP" | "3812" | "5553" | "5224" | "5225" | "5256" | "5262" | "4214" | "4234" | "4245" | "6262"
    )
}

/// The virtual display name for a number: `QPADEV0001`, hexadecimal
/// digits like IBM's, so `QPADEV000A` follows `QPADEV0009`.
#[must_use]
pub fn virtual_name(n: u32) -> String {
    format!("{VIRTUAL_PREFIX}{n:04X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let d = Devd::capture();
        assert_eq!(Devd::decode(&d.encode()).unwrap(), d);
        let mut v = Devd::virtual_display("3179", "2", "USB", true);
        v.status = DevStatus::Active;
        assert_eq!(Devd::decode(&v.encode()).unwrap(), v);
    }

    #[test]
    fn reads_version_one() {
        let mut r = Rec::new(&LAYOUT_V1);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", 1);
        r.set_flag("Online", true);
        r.set_text("DevCls", "*PRT");
        r.set_text("Type", "QSCP");
        let d = Devd::decode(&r.buf).unwrap();
        assert_eq!(d.devtype, "QSCP");
        assert_eq!(d.status, DevStatus::VariedOn);
    }

    #[test]
    fn names_are_hexadecimal() {
        assert_eq!(virtual_name(1), "QPADEV0001");
        assert_eq!(virtual_name(10), "QPADEV000A");
        assert_eq!(virtual_name(16), "QPADEV0010");
    }
}
