// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host print transform: SCS into what an ASCII printer takes.
//! `TRANSFORM(*YES)` with a `MFRTYPMDL` picks a PCL5 model ([`pcl`])
//! or, for `*WSCST`, a customizing object ([`custom`]); the plain text
//! form ([`ascii`]) serves line printers and the `*OTHER` destinations
//! of a remote queue; PDF reuses the capture device's emitter. Every
//! transform is Quartz-written from the published PCL5 command set and
//! unverified against IBM's (`docs/PRINT.md`).

pub mod ascii;
pub mod custom;
pub mod page;
pub mod pcl;

use crate::job::PrintJob;
use crate::wscst::Wscst;

/// An HP Laserjet-class `MFRTYPMDL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    /// `*HPII`: Laserjet II, PCL4.
    HpII,
    /// `*HPIID`: Laserjet IID, PCL4 with a duplexer.
    HpIID,
    /// `*HPIIP`: Laserjet IIP, PCL4.
    HpIIP,
    /// `*HPIII`: Laserjet III, PCL5.
    HpIII,
    /// `*HPIIID`: Laserjet IIID, PCL5 with a duplexer.
    HpIIID,
    /// `*HPIIIP`: Laserjet IIIP, PCL5.
    HpIIIP,
    /// `*HPIIISI`: Laserjet III SI, PCL5 with a duplexer and trays.
    HpIIISI,
    /// `*HPPLUS`: Laserjet Plus, PCL4.
    HpPlus,
    /// `*HP4`: Laserjet 4, PCL 5e at 600 dpi.
    Hp4,
    /// `*HP5`: Laserjet 5, PCL 5e.
    Hp5,
    /// `*HP5SI`: Laserjet 5si, PCL 5e with a duplexer and trays.
    Hp5SI,
}

/// What a model can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// PCL5: scalable fonts and `ESC (s#V`; otherwise PCL4 bitmap fonts
    /// at 10, 12 and 16.67 pitch.
    pub pcl5: bool,
    /// Dots per inch.
    pub dpi: u16,
    /// A duplexer: `ESC &l#S` is sent.
    pub duplex: bool,
    /// More than one input tray: `ESC &l#H` is sent for drawers 2 up.
    pub trays: bool,
}

const MODELS: [(Model, &str, Capabilities); 11] = [
    (Model::HpII, "*HPII", caps(false, 300, false, false)),
    (Model::HpIID, "*HPIID", caps(false, 300, true, true)),
    (Model::HpIIP, "*HPIIP", caps(false, 300, false, false)),
    (Model::HpIII, "*HPIII", caps(true, 300, false, false)),
    (Model::HpIIID, "*HPIIID", caps(true, 300, true, true)),
    (Model::HpIIIP, "*HPIIIP", caps(true, 300, false, false)),
    (Model::HpIIISI, "*HPIIISI", caps(true, 300, true, true)),
    (Model::HpPlus, "*HPPLUS", caps(false, 300, false, false)),
    (Model::Hp4, "*HP4", caps(true, 600, true, true)),
    (Model::Hp5, "*HP5", caps(true, 600, true, true)),
    (Model::Hp5SI, "*HP5SI", caps(true, 600, true, true)),
];

const fn caps(pcl5: bool, dpi: u16, duplex: bool, trays: bool) -> Capabilities {
    Capabilities {
        pcl5,
        dpi,
        duplex,
        trays,
    }
}

impl Model {
    /// From the `MFRTYPMDL` value; `*HPLJ4` is `*HP4`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let up = name.trim().to_ascii_uppercase();
        if up == "*HPLJ4" {
            return Some(Self::Hp4);
        }
        MODELS.iter().find(|(_, n, _)| *n == up).map(|(m, _, _)| *m)
    }

    /// The `MFRTYPMDL` value.
    #[must_use]
    pub fn name(self) -> &'static str {
        MODELS.iter().find(|(m, _, _)| *m == self).map_or("*HP4", |(_, n, _)| n)
    }

    /// What it can do.
    #[must_use]
    pub fn capabilities(self) -> Capabilities {
        MODELS
            .iter()
            .find(|(m, _, _)| *m == self)
            .map_or(caps(true, 600, true, true), |(_, _, c)| *c)
    }

    /// Every model, in table order.
    #[must_use]
    pub fn all() -> Vec<Self> {
        MODELS.iter().map(|(m, _, _)| *m).collect()
    }
}

/// How a file is transformed before it is sent.
#[derive(Clone, Debug, PartialEq)]
pub enum Transform {
    /// Not at all: the SCS goes as it is (`TRANSFORM(*NO)`, or an
    /// IBM-to-IBM transfer).
    Raw,
    /// Plain text with CR LF and form feeds.
    Ascii,
    /// PCL5 for a Laserjet-class model.
    Pcl(Model),
    /// A workstation customizing object's commands.
    Custom(Box<Wscst>),
    /// PDF.
    Pdf,
}

/// What came out of a transform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    /// The bytes to send.
    pub data: Vec<u8>,
    /// The data's format.
    pub format: Format,
}

/// The format of transformed data, as IPP and PJL name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// SCS, untransformed.
    Scs,
    /// Plain text.
    Text,
    /// PCL.
    Pcl,
    /// PDF.
    Pdf,
    /// Whatever a customizing object makes.
    Custom,
}

impl Format {
    /// The IPP `document-format` media type.
    #[must_use]
    pub fn mime(self) -> &'static str {
        match self {
            Self::Scs | Self::Custom => "application/octet-stream",
            Self::Text => "text/plain",
            Self::Pcl => "application/vnd.hp-PCL",
            Self::Pdf => "application/pdf",
        }
    }

    /// The PJL language to enter.
    #[must_use]
    pub fn pjl(self) -> crate::pjl::Language {
        match self {
            Self::Pcl | Self::Text => crate::pjl::Language::Pcl,
            Self::Pdf => crate::pjl::Language::Pdf,
            Self::Scs | Self::Custom => crate::pjl::Language::Auto,
        }
    }
}

impl Transform {
    /// From `TRANSFORM` and `MFRTYPMDL`: `*NO` is raw; `*WSCST` takes
    /// the customizing object; `*NONE` or an unknown model is plain
    /// text; `*PDF` is ours.
    #[must_use]
    pub fn select(transform: bool, mfrtypmdl: &str, wscst: Option<Wscst>) -> Self {
        if !transform {
            return Self::Raw;
        }
        let up = mfrtypmdl.trim().to_ascii_uppercase();
        if up == "*WSCST" {
            return wscst.map_or(Self::Ascii, |w| Self::Custom(Box::new(w)));
        }
        if up == "*PDF" {
            return Self::Pdf;
        }
        Model::parse(&up).map_or(Self::Ascii, Self::Pcl)
    }

    /// Apply to `scs`, the spooled file's data.
    #[must_use]
    pub fn apply(&self, job: &PrintJob, scs: &[u8]) -> Output {
        match self {
            Self::Raw => Output {
                data: scs.to_vec(),
                format: Format::Scs,
            },
            Self::Ascii => Output {
                data: ascii::render(job, scs),
                format: Format::Text,
            },
            Self::Pcl(m) => Output {
                data: pcl::render(*m, job, scs),
                format: Format::Pcl,
            },
            Self::Custom(w) => Output {
                data: custom::render(w, job, scs),
                format: Format::Custom,
            },
            Self::Pdf => {
                let (pages, fmt) = spool::scs::decode(scs);
                Output {
                    data: spool::pdf::render(&pages, fmt),
                    format: Format::Pdf,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_parse() {
        assert_eq!(Model::parse("*hp4"), Some(Model::Hp4));
        assert_eq!(Model::parse("*HPLJ4"), Some(Model::Hp4));
        assert_eq!(Model::parse("*IBM4019"), None);
        assert!(!Model::HpII.capabilities().pcl5);
        assert!(Model::Hp5SI.capabilities().duplex);
        for m in Model::all() {
            assert_eq!(Model::parse(m.name()), Some(m));
        }
    }

    #[test]
    fn selection() {
        assert_eq!(Transform::select(false, "*HP4", None), Transform::Raw);
        assert_eq!(Transform::select(true, "*NONE", None), Transform::Ascii);
        assert_eq!(Transform::select(true, "*HP5", None), Transform::Pcl(Model::Hp5));
        assert_eq!(Transform::select(true, "*WSCST", None), Transform::Ascii);
        assert_eq!(Transform::select(true, "*PDF", None), Transform::Pdf);
        assert_eq!(Format::Pcl.mime(), "application/vnd.hp-PCL");
    }
}
