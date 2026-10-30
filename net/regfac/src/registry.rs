// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility's operations over its points: register,
//! deregister, add and remove exit programs, select. Each refusal is
//! the message IBM's APIs send.

use ipc::wire_struct;

use crate::error::{RegFail, RegResult};
use crate::model::{Desc, ExitPoint, ExitProgram};
use crate::text;

/// Exit program number `*LOW`: the lowest one free.
pub const LOW: i32 = -1;
/// Exit program number `*HIGH`: one past the highest.
pub const HIGH: i32 = -2;
/// The most exit program data a program may carry.
pub const MAX_DATA: usize = 2048;

/// Every exit point, in name and format order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    /// The points.
    pub points: Vec<ExitPoint>,
}

wire_struct!(Registry { points });

/// Which points a selection keeps by registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// Registered points only.
    Registered,
    /// Unregistered points only.
    Unregistered,
    /// Both.
    All,
}

/// How an exit program is added.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddOpts {
    /// Replace a program of the same number.
    pub replace: bool,
    /// Make the exit point, unregistered, when it does not exist.
    pub create_point: bool,
}

wire_struct!(AddOpts {
    replace,
    create_point
});

fn valid_name(s: &str, max: usize) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.len() <= max
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || "$#@".contains(c))
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_$#@.".contains(c))
}

/// `CPF3CD2` unless `name` can name an exit point.
pub fn check_point_name(name: &str) -> RegResult<()> {
    if valid_name(name, 20) {
        Ok(())
    } else {
        Err(RegFail::id("CPF3CD2", format!("{name} cannot name an exit point")).text(name, 20))
    }
}

/// `CPF3CD3` unless `format` can name a format.
pub fn check_format_name(format: &str) -> RegResult<()> {
    if valid_name(format, 8) && !format.contains('.') {
        Ok(())
    } else {
        Err(RegFail::id("CPF3CD3", format!("{format} cannot name a format")).text(format, 8))
    }
}

fn count(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

impl Registry {
    /// The points as shipped.
    #[must_use]
    pub fn shipped() -> Self {
        let mut r = Self {
            points: crate::shipped::points(),
        };
        r.sort();
        r
    }

    fn sort(&mut self) {
        self.points
            .sort_by(|a, b| (&a.name, &a.format).cmp(&(&b.name, &b.format)));
        for p in &mut self.points {
            p.programs.sort_by_key(|g| g.number);
        }
    }

    /// One point.
    #[must_use]
    pub fn point(&self, name: &str, format: &str) -> Option<&ExitPoint> {
        self.points.iter().find(|p| p.is(name, format))
    }

    fn point_mut(&mut self, name: &str, format: &str) -> Option<&mut ExitPoint> {
        self.points.iter_mut().find(|p| p.is(name, format))
    }

    /// The points whose name and format match (`*ALL`, generic or
    /// specific) and whose registration `which` keeps.
    #[must_use]
    pub fn select(&self, name: &str, format: &str, which: Which) -> Vec<&ExitPoint> {
        self.points
            .iter()
            .filter(|p| text::matches(name, &p.name) && text::matches(format, &p.format))
            .filter(|p| match which {
                Which::Registered => p.registered,
                Which::Unregistered => !p.registered,
                Which::All => true,
            })
            .collect()
    }

    /// Register `point` (`QusRegisterExitPoint`). A point that exists
    /// unregistered becomes registered and keeps its programs.
    pub fn register(&mut self, point: ExitPoint) -> RegResult<()> {
        check_point_name(&point.name)?;
        check_format_name(&point.format)?;
        if let Some(p) = self.point_mut(&point.name, &point.format) {
            if p.registered {
                return Err(RegFail::id(
                    "CPF3CD1",
                    format!(
                        "exit point {} format {} is already registered",
                        point.name, point.format
                    ),
                )
                .text(&point.name, 20)
                .text(&point.format, 8));
            }
            if let Some(m) = point.max
                && p.programs.len() > m as usize
            {
                return Err(RegFail::id(
                    "CPF3CD8",
                    format!(
                        "exit point {} already has more programs than {m}",
                        point.name
                    ),
                )
                .text(&point.name, 20)
                .text(&point.format, 8));
            }
            let programs = std::mem::take(&mut p.programs);
            *p = ExitPoint {
                registered: true,
                programs,
                ..point
            };
        } else {
            self.points.push(ExitPoint {
                registered: true,
                programs: Vec::new(),
                ..point
            });
        }
        self.sort();
        Ok(())
    }

    /// Deregister `name` with `format`, or every format (`*ALL`); the
    /// number deregistered.
    pub fn deregister(&mut self, name: &str, format: &str) -> RegResult<usize> {
        let all = format.trim().eq_ignore_ascii_case("*ALL");
        let hits: Vec<usize> = self
            .points
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.name.eq_ignore_ascii_case(name.trim())
                    && (all || p.format.eq_ignore_ascii_case(format.trim()))
            })
            .map(|(i, _)| i)
            .collect();
        if hits.is_empty() {
            return Err(RegFail::no_point(name, format));
        }
        let refused = hits
            .iter()
            .filter(|&&i| !self.points[i].allow_dereg)
            .count();
        if refused > 0 {
            return Err(RegFail::id(
                "CPF3CDC",
                format!("exit point {name} may not be deregistered"),
            )
            .bin4(0)
            .bin4(count(refused)));
        }
        for i in hits.iter().rev() {
            self.points.remove(*i);
        }
        Ok(hits.len())
    }

    /// Add `program` (`QusAddExitProgram`, `ADDEXITPGM`); the number it
    /// got (`LOW` and `HIGH` pick one).
    pub fn add_program(
        &mut self,
        name: &str,
        format: &str,
        mut program: ExitProgram,
        opts: &AddOpts,
    ) -> RegResult<i32> {
        if program.data.len() > MAX_DATA {
            return Err(RegFail::id(
                "CPF3CD6",
                format!(
                    "{} bytes of exit program data is too much",
                    program.data.len()
                ),
            )
            .bin4(count(program.data.len())));
        }
        let pgm_ok = qsname::Name::parse(&program.pgm).is_ok()
            && (program.lib.is_empty()
                || program.lib.starts_with('*')
                || qsname::Name::parse(&program.lib).is_ok());
        if !pgm_ok {
            return Err(RegFail::id(
                "CPF3CDE",
                format!(
                    "{}/{} cannot name an exit program",
                    program.lib, program.pgm
                ),
            )
            .text(&program.pgm, 10)
            .text(&program.lib, 10));
        }
        if program.number < 1 && program.number != LOW && program.number != HIGH {
            return Err(RegFail::id(
                "CPF3CE1",
                format!("{} is not an exit program number", program.number),
            )
            .bin4(program.number));
        }
        if self.point(name, format).is_none() {
            if !opts.create_point {
                return Err(RegFail::no_point(name, format));
            }
            check_point_name(name)?;
            check_format_name(format)?;
            self.points.push(ExitPoint {
                registered: false,
                desc: Desc::default(),
                ..ExitPoint::new(name, format)
            });
        }
        let p = self.point_mut(name, format).expect("made above");
        let number = match program.number {
            LOW => (1..=i32::MAX)
                .find(|n| p.programs.iter().all(|g| g.number != *n))
                .unwrap_or(1),
            HIGH => p
                .programs
                .iter()
                .map(|g| g.number)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
            n => n,
        };
        program.number = number;
        match p.programs.iter().position(|g| g.number == number) {
            Some(i) if opts.replace => p.programs[i] = program,
            Some(_) => {
                return Err(RegFail::id(
                    "CPF3CDF",
                    format!("exit program number {number} is taken on {name} format {format}"),
                )
                .bin4(number)
                .text(name, 20)
                .text(format, 8));
            }
            None => {
                if p.max.is_some_and(|m| p.programs.len() >= m as usize) {
                    return Err(RegFail::id(
                        "CPF3CD4",
                        format!("exit point {name} format {format} has all the programs it takes"),
                    )
                    .text(name, 20)
                    .text(format, 8));
                }
                p.programs.push(program);
            }
        }
        self.sort();
        Ok(number)
    }

    /// Remove exit program `number` (`QusRemoveExitProgram`,
    /// `RMVEXITPGM`); the program removed.
    pub fn remove_program(
        &mut self,
        name: &str,
        format: &str,
        number: i32,
    ) -> RegResult<ExitProgram> {
        let p = self
            .point_mut(name, format)
            .ok_or_else(|| RegFail::no_point(name, format))?;
        let i = p
            .programs
            .iter()
            .position(|g| g.number == number)
            .ok_or_else(|| {
                RegFail::id(
                    "CPF3CDD",
                    format!("exit program number {number} does not exist"),
                )
                .bin4(number)
            })?;
        Ok(p.programs.remove(i))
    }

    /// The programs a caller of `name` with `format` runs, in order.
    #[must_use]
    pub fn programs(&self, name: &str, format: &str) -> Vec<ExitProgram> {
        self.point(name, format)
            .map(|p| p.programs.clone())
            .unwrap_or_default()
    }

    /// Add the shipped points that are missing; how many were added.
    pub fn merge_shipped(&mut self) -> usize {
        let mut added = 0;
        for s in crate::shipped::points() {
            if self.point(&s.name, &s.format).is_none() {
                self.points.push(s);
                added += 1;
            }
        }
        self.sort();
        added
    }
}
