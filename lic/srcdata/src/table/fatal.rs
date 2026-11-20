// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `B1`: fatal; the IPL stops.

use crate::model::{Phase, Src};

/// The IPL program panicked.
pub const PANIC: Src = Src::new(
    "B1001000",
    "slic panic",
    Phase::Fatal,
    "The IPL program panicked.",
)
.cause("A fault in the IPL program; its message follows the code on serial and the console.");
/// LIC partition not found.
pub const LIC_NOT_FOUND: Src = Src::new(
    "B1001010",
    "LIC partition not found",
    Phase::Fatal,
    "LIC partition not found.",
)
.cause("No LIC partition on the disks, or it would not open; the serial log names the error.")
.recovery("IPL from installation media: a D IPL runs without a LIC partition.");
/// LIC record unreadable in both copies.
pub const LIC_UNREADABLE: Src = Src::new(
    "B1001020",
    "LIC record unreadable, both copies",
    Phase::Fatal,
    "LIC record unreadable in both copies.",
)
.cause("Neither copy of the LIC record could be read, or the record could not be written.");
/// A disk unit the ASP needs is not there.
pub const ASP_NOT_FOUND: Src = Src::new(
    "B1001030",
    "Disk unit missing",
    Phase::Fatal,
    "A disk unit the ASP needs is not there: no ASP 1 partition at all, an unprotected \
     unit, or both halves of a pair.",
)
.cause(
    "A unit ASP 1 cannot run without is missing, including more parity members than the \
     set can lose or any member of a set still synchronizing.",
)
.recovery("A manual IPL stops in DST to work with the disk units.");
/// The store could not be opened or initialized.
pub const ASP_UNUSABLE: Src = Src::new(
    "B1001040",
    "Object store unusable",
    Phase::Fatal,
    "The store could not be opened or initialized.",
)
.cause(
    "ASP 1 could not be formatted, opened or synchronized (C1001085 before it), or an \
     automatic operating system install failed; the serial log names the error.",
);
/// The store belongs to another release.
pub const ASP_WRONG_RELEASE: Src = Src::new(
    "B1001050",
    "Object store created by another release",
    Phase::Fatal,
    "The store belongs to another release.",
)
.cause("ASP 1 was created by another release (C1001087 names both).")
.recovery("An older release's store is upgraded by installing the new release over it.");
/// The storage service process ended while the system ran.
pub const STG_ENDED: Src = Src::new(
    "B1001060",
    "Storage management service ended",
    Phase::Fatal,
    "The storage service process ended while the system ran.",
)
.cause("The storage service process ended; the serial log names its signal or exit code.")
.recovery("IPL again: the store recovers from its log as after any crash.");
