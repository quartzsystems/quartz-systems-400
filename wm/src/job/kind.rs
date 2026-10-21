// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job types and subtypes, as `QUSRJOBI` codes them and `WRKACTJOB`
//! shows them.

/// The job type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobType {
    /// `A`: autostart.
    Autostart,
    /// `B`: batch.
    Batch,
    /// `I`: interactive.
    Interactive,
    /// `M`: subsystem monitor.
    Monitor,
    /// `R`: spooled reader.
    Reader,
    /// `S`: system job.
    System,
    /// `W`: spooled writer.
    Writer,
    /// `X`: the start-control-program-function job, `SCPF`.
    Scpf,
}

/// The job subtype.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SubType {
    /// None.
    #[default]
    None,
    /// `D`: batch immediate.
    Immediate,
    /// `E`: started by a procedure start request.
    Evoked,
    /// `J`: prestart.
    Prestart,
    /// `P`: print driver.
    PrintDriver,
    /// `T`: multiple requester terminal.
    Mrt,
    /// `U`: alternate spool user.
    SpoolUser,
}

impl JobType {
    /// The one-letter code.
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Self::Autostart => 'A',
            Self::Batch => 'B',
            Self::Interactive => 'I',
            Self::Monitor => 'M',
            Self::Reader => 'R',
            Self::System => 'S',
            Self::Writer => 'W',
            Self::Scpf => 'X',
        }
    }

    /// From the one-letter code.
    #[must_use]
    pub fn from_code(c: char) -> Option<Self> {
        Some(match c {
            'A' => Self::Autostart,
            'B' => Self::Batch,
            'I' => Self::Interactive,
            'M' => Self::Monitor,
            'R' => Self::Reader,
            'S' => Self::System,
            'W' => Self::Writer,
            'X' => Self::Scpf,
            _ => return None,
        })
    }
}

impl SubType {
    /// The one-letter code, blank for none.
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Self::None => ' ',
            Self::Immediate => 'D',
            Self::Evoked => 'E',
            Self::Prestart => 'J',
            Self::PrintDriver => 'P',
            Self::Mrt => 'T',
            Self::SpoolUser => 'U',
        }
    }

    /// From the one-letter code.
    #[must_use]
    pub fn from_code(c: char) -> Option<Self> {
        Some(match c {
            ' ' => Self::None,
            'D' => Self::Immediate,
            'E' => Self::Evoked,
            'J' => Self::Prestart,
            'P' => Self::PrintDriver,
            'T' => Self::Mrt,
            'U' => Self::SpoolUser,
            _ => return None,
        })
    }
}

/// The `WRKACTJOB` type column: `ASJ`, `BCH`, `BCI`, `EVK`, `INT`,
/// `M36`, `MRT`, `PJ`, `PDJ`, `RDR`, `SBS`, `SYS`, `WTR`.
#[must_use]
pub fn wrkactjob_type(t: JobType, s: SubType) -> &'static str {
    match (t, s) {
        (JobType::Autostart, _) => "ASJ",
        (JobType::Batch, SubType::Immediate) => "BCI",
        (JobType::Batch, SubType::Evoked) => "EVK",
        (JobType::Batch, SubType::Prestart) => "PJ",
        (JobType::Batch, SubType::Mrt) => "MRT",
        (JobType::Batch, _) => "BCH",
        (JobType::Interactive, SubType::Mrt) => "M36",
        (JobType::Interactive, _) => "INT",
        (JobType::Monitor, _) => "SBS",
        (JobType::Reader, _) => "RDR",
        (JobType::Writer, SubType::PrintDriver) => "PDJ",
        (JobType::Writer, _) => "WTR",
        (JobType::System | JobType::Scpf, _) => "SYS",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        for c in "ABIMRSWX".chars() {
            assert_eq!(JobType::from_code(c).unwrap().code(), c);
        }
        for c in " DEJPTU".chars() {
            assert_eq!(SubType::from_code(c).unwrap().code(), c);
        }
        assert_eq!(wrkactjob_type(JobType::Batch, SubType::Immediate), "BCI");
        assert_eq!(wrkactjob_type(JobType::Monitor, SubType::None), "SBS");
    }
}
