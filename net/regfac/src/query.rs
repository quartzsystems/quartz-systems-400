// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The questions a job asks the registration facility, on the wire
//! between a job and the storage service: the APIs', the commands' and
//! the servers'. [`answer`] runs one over a [`Registry`]; the service
//! loads and saves the repository around it, checks authority and
//! writes the audit entries it returns.

use ipc::wire_enum;
use sec::audit::{Event, Kind};

use crate::call::Called;
use crate::error::RegResult;
use crate::model::{ExitPoint, ExitProgram};
use crate::registry::{AddOpts, Registry, Which};

/// A question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    /// The points matching `name` and `format` (`*ALL`, generic,
    /// specific): `which` 0 registered, 1 unregistered, 2 both.
    List {
        /// Exit point name.
        name: String,
        /// Format.
        format: String,
        /// Registration filter.
        which: u8,
    },
    /// The programs a server calls for a point.
    Programs {
        /// Exit point name.
        name: String,
        /// Format.
        format: String,
    },
    /// Register a point (`QUSRGPT`).
    Register {
        /// The point, without programs.
        point: ExitPoint,
    },
    /// Deregister a point (`QUSDRGPT`); `*ALL` formats.
    Deregister {
        /// Exit point name.
        name: String,
        /// Format.
        format: String,
    },
    /// Add an exit program (`QUSADDEP`, `ADDEXITPGM`).
    Add {
        /// Exit point name.
        name: String,
        /// Format.
        format: String,
        /// The program; number `LOW` or `HIGH` picks one.
        program: ExitProgram,
        /// Replace and create.
        opts: AddOpts,
    },
    /// Remove an exit program (`QUSRMVEP`, `RMVEXITPGM`).
    Remove {
        /// Exit point name.
        name: String,
        /// Format.
        format: String,
        /// Program number.
        number: i32,
    },
    /// An exit program refused a request or failed: audit it.
    Refused {
        /// The call.
        call: Called,
    },
}

wire_enum!(Query {
    0 => List { name, format, which },
    1 => Programs { name, format },
    2 => Register { point },
    3 => Deregister { name, format },
    4 => Add { name, format, program, opts },
    5 => Remove { name, format, number },
    6 => Refused { call },
});

/// An answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Points, with their programs.
    Points {
        /// The points.
        points: Vec<ExitPoint>,
    },
    /// Programs.
    Programs {
        /// The programs.
        programs: Vec<ExitProgram>,
    },
    /// A count or a program number.
    Number {
        /// The number.
        n: i32,
    },
    /// Done.
    Done,
}

wire_enum!(Answer {
    0 => Points { points },
    1 => Programs { programs },
    2 => Number { n },
    3 => Done,
});

impl Query {
    /// Whether the query changes the repository (and needs the
    /// authority to).
    #[must_use]
    pub fn changes(&self) -> bool {
        matches!(
            self,
            Self::Register { .. }
                | Self::Deregister { .. }
                | Self::Add { .. }
                | Self::Remove { .. }
        )
    }
}

/// What a query did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The answer.
    pub answer: Answer,
    /// Whether the registry changed.
    pub changed: bool,
    /// Audit entries (`GR`) to write.
    pub audit: Vec<Event>,
}

fn which(n: u8) -> Which {
    match n {
        0 => Which::Registered,
        1 => Which::Unregistered,
        _ => Which::All,
    }
}

fn gr(kind: &str, point: &str, pgm: &str, lib: &str, rc: &str) -> Event {
    Event::new(Kind::Gr)
        .set("ETYP", kind)
        .set("EXPT", point)
        .set("EXPN", pgm)
        .set("EXPL", lib)
        .set("RTCD", rc)
}

/// Answer `q` over `reg`.
pub fn answer(reg: &mut Registry, q: Query) -> RegResult<Outcome> {
    let done = |answer: Answer, changed: bool, audit: Vec<Event>| Outcome {
        answer,
        changed,
        audit,
    };
    Ok(match q {
        Query::List {
            name,
            format,
            which: w,
        } => {
            let points = reg
                .select(&name, &format, which(w))
                .into_iter()
                .cloned()
                .collect();
            done(Answer::Points { points }, false, Vec::new())
        }
        Query::Programs { name, format } => done(
            Answer::Programs {
                programs: reg.programs(&name, &format),
            },
            false,
            Vec::new(),
        ),
        Query::Register { point } => {
            reg.register(point)?;
            done(Answer::Done, true, Vec::new())
        }
        Query::Deregister { name, format } => {
            let n = reg.deregister(&name, &format)?;
            done(
                Answer::Number {
                    n: i32::try_from(n).unwrap_or(i32::MAX),
                },
                true,
                Vec::new(),
            )
        }
        Query::Add {
            name,
            format,
            program,
            opts,
        } => {
            let replacing = opts.replace
                && reg
                    .point(&name, &format)
                    .is_some_and(|p| p.programs.iter().any(|g| g.number == program.number));
            let (pgm, lib) = (program.pgm.clone(), program.lib.clone());
            let n = reg.add_program(&name, &format, program, &opts)?;
            let kind = if replacing { "R" } else { "A" };
            done(
                Answer::Number { n },
                true,
                vec![gr(kind, &name, &pgm, &lib, "")],
            )
        }
        Query::Remove {
            name,
            format,
            number,
        } => {
            let g = reg.remove_program(&name, &format, number)?;
            let audit = vec![gr("D", &name, &g.pgm, &g.lib, "")];
            done(Answer::Done, true, audit)
        }
        Query::Refused { call } => {
            let audit = vec![gr("F", &call.point, &call.pgm, &call.lib, &call.rc)];
            done(Answer::Done, false, audit)
        }
    })
}
