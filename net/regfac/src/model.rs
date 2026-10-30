// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Exit points and exit programs as the registration facility keeps
//! them.

use ipc::{wire_enum, wire_struct};

/// A description: a text, or a message whose first level is the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Desc {
    /// Text, up to 50 characters (indicator `T`).
    Text {
        /// The text.
        text: String,
    },
    /// A message description (indicator `M`).
    Msg {
        /// Message file.
        msgf: String,
        /// Its library.
        lib: String,
        /// Message identifier.
        msgid: String,
    },
}

wire_enum!(Desc {
    0 => Text { text },
    1 => Msg { msgf, lib, msgid },
});

impl Default for Desc {
    fn default() -> Self {
        Self::Text {
            text: String::new(),
        }
    }
}

impl Desc {
    /// A text description.
    #[must_use]
    pub fn text(s: &str) -> Self {
        Self::Text { text: s.into() }
    }

    /// The text, empty for a message description.
    #[must_use]
    pub fn as_text(&self) -> &str {
        match self {
            Self::Text { text } => text,
            Self::Msg { .. } => "",
        }
    }

    /// The EXTI indicator: `T` or `M`.
    #[must_use]
    pub fn indicator(&self) -> char {
        match self {
            Self::Text { .. } => 'T',
            Self::Msg { .. } => 'M',
        }
    }
}

/// A preprocessing exit program the facility names for an operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prep {
    /// Program.
    pub pgm: String,
    /// Library.
    pub lib: String,
    /// Format (`ADDP0100`, `RMVP0100`, `RTVP0100`).
    pub format: String,
}

wire_struct!(Prep { pgm, lib, format });

impl Prep {
    /// `lib/pgm` with `format`.
    #[must_use]
    pub fn new(lib: &str, pgm: &str, format: &str) -> Self {
        Self {
            pgm: pgm.into(),
            lib: lib.into(),
            format: format.into(),
        }
    }
}

/// Whether an exit program is threadsafe (`THDSAFE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThreadSafe {
    /// `*UNKNOWN` (`0`).
    #[default]
    Unknown,
    /// `*NO` (`1`).
    No,
    /// `*YES` (`2`).
    Yes,
}

wire_enum!(ThreadSafe {
    0 => Unknown,
    1 => No,
    2 => Yes,
});

impl ThreadSafe {
    /// The EXTI code.
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Self::Unknown => '0',
            Self::No => '1',
            Self::Yes => '2',
        }
    }

    /// From the EXTI code.
    #[must_use]
    pub fn of_code(c: char) -> Self {
        match c {
            '1' => Self::No,
            '2' => Self::Yes,
            _ => Self::Unknown,
        }
    }

    /// The special value.
    #[must_use]
    pub fn special(self) -> &'static str {
        match self {
            Self::Unknown => "*UNKNOWN",
            Self::No => "*NO",
            Self::Yes => "*YES",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn of_special(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*UNKNOWN" | "" => Some(Self::Unknown),
            "*NO" => Some(Self::No),
            "*YES" => Some(Self::Yes),
            _ => None,
        }
    }
}

/// What a multithreaded job does with the program (`MLTTHDACN`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MtAction {
    /// `*SYSVAL` (`0`): `QMLTTHDACN` decides.
    #[default]
    SysVal,
    /// `*RUN` (`1`).
    Run,
    /// `*MSG` (`2`): run and send `CPI3C80`.
    Msg,
    /// `*NORUN` (`3`).
    NoRun,
}

wire_enum!(MtAction {
    0 => SysVal,
    1 => Run,
    2 => Msg,
    3 => NoRun,
});

impl MtAction {
    /// The EXTI code.
    #[must_use]
    pub fn code(self) -> char {
        match self {
            Self::SysVal => '0',
            Self::Run => '1',
            Self::Msg => '2',
            Self::NoRun => '3',
        }
    }

    /// From the EXTI code.
    #[must_use]
    pub fn of_code(c: char) -> Self {
        match c {
            '1' => Self::Run,
            '2' => Self::Msg,
            '3' => Self::NoRun,
            _ => Self::SysVal,
        }
    }

    /// The special value.
    #[must_use]
    pub fn special(self) -> &'static str {
        match self {
            Self::SysVal => "*SYSVAL",
            Self::Run => "*RUN",
            Self::Msg => "*MSG",
            Self::NoRun => "*NORUN",
        }
    }

    /// From the special value.
    #[must_use]
    pub fn of_special(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*SYSVAL" | "" => Some(Self::SysVal),
            "*RUN" => Some(Self::Run),
            "*MSG" => Some(Self::Msg),
            "*NORUN" => Some(Self::NoRun),
            _ => None,
        }
    }
}

/// One exit program of an exit point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitProgram {
    /// Its number: the order programs are called in, from 1.
    pub number: i32,
    /// Program.
    pub pgm: String,
    /// Library.
    pub lib: String,
    /// Exit program data, handed to whoever calls it.
    pub data: Vec<u8>,
    /// The data's CCSID; 0 is the job's.
    pub ccsid: i32,
    /// `THDSAFE`.
    pub threadsafe: ThreadSafe,
    /// `MLTTHDACN`.
    pub mt_action: MtAction,
    /// Its description.
    pub desc: Desc,
}

wire_struct!(ExitProgram {
    number,
    pgm,
    lib,
    data,
    ccsid,
    threadsafe,
    mt_action,
    desc
});

/// One exit point and format, with its programs in number order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitPoint {
    /// Exit point name, up to 20.
    pub name: String,
    /// Format name, 8.
    pub format: String,
    /// Whether it is registered; an exit program added with
    /// `CRTEXITPNT(*YES)` makes an unregistered one.
    pub registered: bool,
    /// Whether it may be deregistered.
    pub allow_dereg: bool,
    /// Whether its information may be changed.
    pub allow_change: bool,
    /// The most programs it takes; `None` is no maximum.
    pub max: Option<u32>,
    /// Preprocessing for add.
    pub prep_add: Option<Prep>,
    /// Preprocessing for remove.
    pub prep_rmv: Option<Prep>,
    /// Preprocessing for retrieve.
    pub prep_rtv: Option<Prep>,
    /// Its description.
    pub desc: Desc,
    /// Its programs, by number.
    pub programs: Vec<ExitProgram>,
}

wire_struct!(ExitPoint {
    name,
    format,
    registered,
    allow_dereg,
    allow_change,
    max,
    prep_add,
    prep_rmv,
    prep_rtv,
    desc,
    programs
});

impl ExitPoint {
    /// A registered point with no programs and no maximum, allowing
    /// deregistration and change.
    #[must_use]
    pub fn new(name: &str, format: &str) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            format: format.to_ascii_uppercase(),
            registered: true,
            allow_dereg: true,
            allow_change: true,
            ..Self::default()
        }
    }

    /// The EXTI maximum: -1 for no maximum.
    #[must_use]
    pub fn max_code(&self) -> i32 {
        self.max
            .map_or(-1, |m| i32::try_from(m).unwrap_or(i32::MAX))
    }

    /// Whether `(name, format)` is this point.
    #[must_use]
    pub fn is(&self, name: &str, format: &str) -> bool {
        self.name.eq_ignore_ascii_case(name.trim())
            && self.format.eq_ignore_ascii_case(format.trim())
    }
}
