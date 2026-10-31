// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The tape class of save and restore: one label-and-command layer over a
//! transport trait.
//!
//! - [`transport`]: [`TapeTransport`] (a drive) and [`Changer`] (a media
//!   library's robot), the only things the layers above touch.
//! - [`virt`]: [`VirtualTape`], a volume in one file, and
//!   [`VirtualLibrary`], volumes in slots plus drives.
//! - [`st`]: the Linux `st` tape driver and SCSI media changers over `sg`,
//!   each behind a small ioctl trait so tests replay a transcript.
//! - [`label`]: IBM standard labels (VOL1, HDR1/2, EOF1/2, EOV1/2).
//! - [`volume`]: the operations of INZTAP, CHKTAP, DUPTAP, DSPTAP and the
//!   data part of CPYTOTAP/CPYFRMTAP, with multi-volume files through a
//!   [`VolumeProvider`].
//! - [`device`]: `TAPnn` and `TAPMLBnn` resource names and the *TAPMLB
//!   cartridge model behind WRKTAPCTG.
//!
//! The volume file format and the label layouts are specified in
//! `docs/MEDIA.md`; [`spec`] holds the code's side of those tables.

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod device;
pub mod error;
pub mod label;
pub mod spec;
pub mod st;
pub mod transport;
pub mod virt;
pub mod volume;

pub use error::TapeError;
pub use transport::{
    Changer, ChangerError, Element, ElementKind, MAX_BLOCK, Position, ReadOutcome, Status,
    TapeTransport, TransportError, Written,
};
pub use virt::{VirtualLibrary, VirtualTape, VolumeOptions};
pub use volume::{EndOpt, SeqNbr, VolumeProvider, VolumeRequest};
