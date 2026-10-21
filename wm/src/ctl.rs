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
    /// Type code (`QUSRJOBI`).
    pub kind: char,
    /// The subsystem it runs in; blank for none.
    pub subsystem: String,
    /// The job's role: `QSVCJOB` serves the service console,
    /// `MONITOR` runs a subsystem, `BATCH` runs a routed job.
    pub role: String,
    /// The job description, `lib/name`.
    pub jobd: String,
    /// The class routing gave it, `lib/name`.
    pub class: String,
    /// The program routing calls first, `lib/name`; `QSYS/QCMD` runs
    /// the request data.
    pub program: String,
    /// The request data: a command for `QCMD`.
    pub rqsdta: String,
    /// The pool it runs in: `*BASE`, `*SHRPOOLn`, or `SBS/n` for a
    /// subsystem's private pool.
    pub pool: String,
    /// The `*LDA` it starts with; empty for blanks.
    pub lda: Vec<u8>,
}

wire_struct!(Start {
    name,
    user,
    number,
    handle,
    kind,
    subsystem,
    role,
    jobd,
    class,
    program,
    rqsdta,
    pool,
    lda
});

/// Parent to job.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToJob {
    /// Who the job is; the first message.
    Start {
        /// The attributes.
        start: Box<Start>,
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
    /// The job's display answered: Enter (key 0) or a function key,
    /// and every input field.
    Input {
        /// 0 for Enter, else the function key number.
        key: u8,
        /// The fields in order.
        fields: Vec<String>,
    },
}

wire_enum!(ToJob {
    0 => Start { start },
    1 => Line { text },
    2 => End { immediate },
    3 => Input { key, fields },
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
    /// Show a panel on the job's display.
    Show {
        /// The panel.
        panel: Panel,
    },
    /// `PWRDWNSYS`: power the system down.
    PowerDown {
        /// `OPTION(*IMMED)`.
        immediate: bool,
        /// `DELAY` seconds for `*CNTRLD`.
        delay: u32,
        /// `RESTART(*YES)`.
        restart: bool,
    },
}

wire_enum!(FromJob {
    0 => Ready { pid },
    1 => Output { text },
    2 => Done { prompt, srcs, sysvals },
    3 => Ending { reason },
    4 => Show { panel },
    5 => PowerDown { immediate, delay, restart },
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
    "dspjoblog",
    "strsbs",
    "dlyjob",
    "sbmjob",
    "pwrdwnsys",
    "endsys",
    "wrkjob",
];

/// A display panel: text and input fields placed by row and column
/// (1-based), and the message line. `MIGRATE(phase8)`: display files
/// build these.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Panel {
    /// Text: row, column, attribute (0 green, 1 white, 2 blue, 3 red),
    /// the text.
    pub texts: Vec<(u8, u8, u8, String)>,
    /// Input fields: row, column, length, hidden, initial value.
    pub fields: Vec<(u8, u8, u16, bool, String)>,
    /// The message line.
    pub message: String,
    /// The message is an error.
    pub error: bool,
}

wire_struct!(Panel {
    texts,
    fields,
    message,
    error
});

impl Panel {
    /// Put text.
    pub fn text(&mut self, row: u8, col: u8, attr: u8, s: impl Into<String>) {
        self.texts.push((row, col, attr, s.into()));
    }

    /// Put an input field.
    pub fn field(&mut self, row: u8, col: u8, len: u16, value: impl Into<String>) {
        self.fields.push((row, col, len, false, value.into()));
    }

    /// The panel as 24 lines of 80, for tests and dumps.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut grid = vec![vec![' '; 80]; 24];
        let mut put = |row: u8, col: u8, s: &str| {
            let r = usize::from(row.max(1) - 1).min(23);
            for (i, ch) in s.chars().enumerate() {
                let c = usize::from(col.max(1) - 1) + i;
                if c < 80 {
                    grid[r][c] = ch;
                }
            }
        };
        for (r, c, _, s) in &self.texts {
            put(*r, *c, s);
        }
        for (r, c, _, hidden, v) in &self.fields {
            if !hidden {
                put(*r, *c, v);
            }
        }
        if !self.message.is_empty() {
            put(23, 1, &self.message);
        }
        grid.into_iter().map(|l| l.into_iter().collect()).collect()
    }
}
