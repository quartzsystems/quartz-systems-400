// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A media library's robot: elements and the moves between them.

/// The element types of SMC-3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ElementKind {
    /// The medium transport (the picker).
    Transport,
    /// A storage slot.
    Storage,
    /// An import/export station (mail slot).
    ImportExport,
    /// A data transfer element (a drive).
    Drive,
}

impl ElementKind {
    /// The SMC element type code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Transport => 1,
            Self::Storage => 2,
            Self::ImportExport => 3,
            Self::Drive => 4,
        }
    }

    /// The kind for an SMC element type code.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Transport),
            2 => Some(Self::Storage),
            3 => Some(Self::ImportExport),
            4 => Some(Self::Drive),
            _ => None,
        }
    }
}

/// One element's status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// Its type.
    pub kind: ElementKind,
    /// Its element address.
    pub address: u16,
    /// A cartridge is in it.
    pub full: bool,
    /// The cartridge's volume tag (barcode), when it has one.
    pub volume_tag: Option<String>,
    /// The element the cartridge came from, when the changer knows.
    pub source: Option<u16>,
}

/// A changer failed an operation.
#[derive(Debug, thiserror::Error)]
pub enum ChangerError {
    /// No element has this address.
    #[error("no element at address {0}")]
    NoElement(u16),
    /// The source element is empty.
    #[error("element {0} is empty")]
    SourceEmpty(u16),
    /// The destination element is full.
    #[error("element {0} is full")]
    DestinationFull(u16),
    /// The changer answered a command with a check condition.
    #[error("changer check condition: sense key {key:#x}, ASC {asc:#04x}, ASCQ {ascq:#04x}")]
    Check {
        /// Sense key.
        key: u8,
        /// Additional sense code.
        asc: u8,
        /// Its qualifier.
        ascq: u8,
    },
    /// The element status data does not parse.
    #[error("bad element status data: {0}")]
    Format(String),
    /// The device failed.
    #[error("changer device error: {0}")]
    Io(#[from] std::io::Error),
}

/// A media changer.
pub trait Changer {
    /// Move the cartridge in element `from` to element `to`.
    fn move_medium(&mut self, from: u16, to: u16) -> Result<(), ChangerError>;
    /// Every element's status, with volume tags.
    fn read_element_status(&mut self) -> Result<Vec<Element>, ChangerError>;
}
