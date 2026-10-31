// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The label layer's errors and the IBM message identifiers they map to.

use crate::label::{Date, LabelError};
use crate::transport::{ChangerError, TransportError};

/// A tape operation failed.
#[derive(Debug, thiserror::Error)]
pub enum TapeError {
    /// The drive failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The changer failed.
    #[error(transparent)]
    Changer(#[from] ChangerError),
    /// A label is damaged or not what the position should hold.
    #[error("label error: {0}")]
    Label(#[from] LabelError),
    /// The mounted volume is not the one asked for.
    #[error("volume {found} is mounted where volume {expected} was expected")]
    WrongVolume {
        /// The volume asked for.
        expected: String,
        /// The volume found ("*NL" when unlabeled, "*BLANK" when empty).
        found: String,
    },
    /// The operation needs a labeled volume.
    #[error("the volume has no standard labels")]
    NotLabeled,
    /// The operation needs an unlabeled volume, or a labeled one was in the way.
    #[error("the volume has standard labels; an unlabeled volume was expected")]
    Labeled,
    /// No file with that sequence number or label.
    #[error("file {what} was not found on the volume")]
    FileNotFound {
        /// The sequence number or label looked for.
        what: String,
    },
    /// Writing would destroy a file that has not expired.
    #[error("file {seq} ({label}) on volume {volume} is protected until {expires}")]
    ActiveFile {
        /// The volume.
        volume: String,
        /// Its sequence number.
        seq: u32,
        /// Its file label.
        label: String,
        /// When it expires ("*PERM" for never).
        expires: String,
    },
    /// The volume is full and no further volume was mounted.
    #[error("end of volume reached and no further volume is available")]
    EndOfVolume,
    /// The volume provider declined to mount another volume.
    #[error("the request for another volume was cancelled")]
    Cancelled,
    /// A continuation volume does not continue the file.
    #[error("volume {volume} does not continue file {file} (found {found})")]
    NotContinuation {
        /// The volume mounted.
        volume: String,
        /// The file being read.
        file: String,
        /// What was found instead.
        found: String,
    },
    /// A parameter value is not valid.
    #[error("parameter {param}: {why}")]
    Parameter {
        /// The parameter keyword.
        param: &'static str,
        /// Why it was refused.
        why: String,
    },
    /// Records do not fit the file's record or block length.
    #[error("record of {len} bytes does not fit record length {max}")]
    RecordLength {
        /// The record's length.
        len: usize,
        /// The limit.
        max: usize,
    },
}

/// The display form of an expiration for messages.
#[must_use]
pub fn expiry_text(d: Option<Date>) -> String {
    d.map_or_else(
        || "*PERM".to_string(),
        |d| format!("{}.{:03}", d.year, d.day),
    )
}

impl TapeError {
    /// The IBM message identifier the commands send for this error.
    #[must_use]
    pub fn message_id(&self) -> &'static str {
        match self {
            Self::Transport(t) => match t {
                TransportError::NotLoaded => "CPF6760",
                TransportError::WriteProtected => "CPF6768",
                TransportError::EndOfMedium => "CPF6718",
                _ => "CPF6708",
            },
            Self::Changer(_) => "CPF6745",
            Self::Label(_) | Self::NotLabeled | Self::Labeled => "CPF6772",
            Self::WrongVolume { .. } | Self::NotContinuation { .. } => "CPF6720",
            Self::FileNotFound { .. } => "CPF6790",
            Self::ActiveFile { .. } => "CPA4036",
            Self::EndOfVolume => "CPF6718",
            Self::Cancelled | Self::Parameter { .. } | Self::RecordLength { .. } => "CPF6708",
        }
    }
}
