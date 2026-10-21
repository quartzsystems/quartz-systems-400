// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a job is: its overall state (`*ACTIVE`, `*JOBQ`, `*OUTQ`) and,
//! while active, the four-letter status `WRKACTJOB` shows. The runtime
//! sets the active status at every point a job blocks.

/// The overall state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobState {
    /// On a job queue, waiting to start.
    JobQ,
    /// Running.
    Active,
    /// Ended, its spooled output still on an output queue.
    OutQ,
}

impl JobState {
    /// `*ACTIVE`, `*JOBQ`, `*OUTQ`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::JobQ => "*JOBQ",
            Self::Active => "*ACTIVE",
            Self::OutQ => "*OUTQ",
        }
    }
}

/// An active job's status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Status {
    /// Running, or ready to run.
    #[default]
    Run,
    /// Waiting on a data queue.
    Deqw,
    /// Waiting for a reply to a message.
    Msgw,
    /// Waiting for input at a display.
    Dspw,
    /// Waiting for a lock.
    Lckw,
    /// Waiting for a time interval.
    Timw,
    /// Waiting for an event.
    Evtw,
    /// Waiting in a select (sockets).
    Selw,
    /// Waiting for communications.
    Cmnw,
    /// Delayed by `DLYJOB`.
    Dlyw,
    /// Held.
    Hld,
    /// Ending.
    End,
    /// Waiting for an activity level.
    Inel,
}

impl Status {
    /// The four letters.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Run => "RUN",
            Self::Deqw => "DEQW",
            Self::Msgw => "MSGW",
            Self::Dspw => "DSPW",
            Self::Lckw => "LCKW",
            Self::Timw => "TIMW",
            Self::Evtw => "EVTW",
            Self::Selw => "SELW",
            Self::Cmnw => "CMNW",
            Self::Dlyw => "DLYW",
            Self::Hld => "HLD",
            Self::End => "END",
            Self::Inel => "INEL",
        }
    }

    /// From the letters.
    #[must_use]
    pub fn from_code(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.code() == s)
    }

    /// Every status.
    pub const ALL: [Self; 13] = [
        Self::Run,
        Self::Deqw,
        Self::Msgw,
        Self::Dspw,
        Self::Lckw,
        Self::Timw,
        Self::Evtw,
        Self::Selw,
        Self::Cmnw,
        Self::Dlyw,
        Self::Hld,
        Self::End,
        Self::Inel,
    ];

    /// The byte a job's shared status word carries.
    #[must_use]
    pub fn byte(self) -> u8 {
        u8::try_from(Self::ALL.iter().position(|s| *s == self).unwrap_or(0)).unwrap_or(0)
    }

    /// From that byte.
    #[must_use]
    pub fn from_byte(b: u8) -> Self {
        Self::ALL.get(usize::from(b)).copied().unwrap_or_default()
    }

    /// Whether the job holds an activity level in this status: waits
    /// give it up.
    #[must_use]
    pub fn runnable(self) -> bool {
        matches!(self, Self::Run | Self::End)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        for s in Status::ALL {
            assert_eq!(Status::from_code(s.code()), Some(s));
            assert_eq!(Status::from_byte(s.byte()), s);
        }
        assert_eq!(JobState::OutQ.name(), "*OUTQ");
    }
}
