// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The control channel between a job process and the process that
//! started it. The parent passes its end as the job's standard input;
//! the first message tells the job who it is.

use ipc::{wire_enum, wire_struct};

/// What a job is started as.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Start {
    /// Job name.
    pub name: String,
    /// User name.
    pub user: String,
    /// Job number.
    pub number: u32,
    /// The lock owner handle.
    pub handle: u64,
    /// The job's role: `QSVCJOB` serves the service console.
    pub role: String,
}

wire_struct!(Start {
    name,
    user,
    number,
    handle,
    role
});

/// Parent to job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToJob {
    /// Who the job is; the first message.
    Start {
        /// The attributes.
        start: Start,
    },
    /// A line typed at the console the job serves.
    Line {
        /// The text.
        text: String,
    },
    /// End: `immediate` skips cleanup the job would otherwise do.
    End {
        /// `*IMMED`.
        immediate: bool,
    },
}

wire_enum!(ToJob {
    0 => Start { start },
    1 => Line { text },
    2 => End { immediate },
});

/// Job to parent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromJob {
    /// Started and connected to the storage service.
    Ready {
        /// The process ID.
        pid: u32,
    },
    /// Text for the console.
    Output {
        /// The text.
        text: String,
    },
    /// A line is done: the prompt to show, SRC codes the work raised,
    /// and the system values when they changed.
    Done {
        /// The prompt.
        prompt: String,
        /// SRC codes.
        srcs: Vec<String>,
        /// The system values.
        sysvals: Option<Vec<u8>>,
    },
    /// The job is ending.
    Ending {
        /// Why, for the log.
        reason: String,
    },
}

wire_enum!(FromJob {
    0 => Ready { pid },
    1 => Output { text },
    2 => Done { prompt, srcs, sysvals },
    3 => Ending { reason },
});

/// The service console's prompt.
pub const PROMPT: &str = "QS400-SVC> ";

/// The service console's commands the job runs itself: calls, the
/// job's displays and the debugger. The storage service runs the rest.
pub const JOB_COMMANDS: &[&str] = &[
    "call",
    "dspactgrp",
    "rclactgrp",
    "dspcallstk",
    "dspmsgq",
    "rpy",
    "dmp",
    "strdbg",
    "enddbg",
    "addbkp",
    "rmvbkp",
    "dspbkp",
];
