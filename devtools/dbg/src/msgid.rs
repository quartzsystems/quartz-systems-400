// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages the debug session answers with: identifiers of
//! `QCPFMSG`: IBM's debugger messages (`CPF9Cnn`, read with F1 and
//! DSPMSGD on pub400, V7R5M0) where IBM has one, Quartz's otherwise,
//! worded by the job's driver from the message file.

/// A message for the message line: its identifier and values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    /// The message identifier.
    pub id: &'static str,
    /// The values, in the order of the description's variables.
    pub data: Vec<String>,
}

impl Msg {
    /// Message `id` with `data`.
    #[must_use]
    pub fn new(id: &'static str, data: &[&dyn std::fmt::Display]) -> Self {
        Self {
            id,
            data: data.iter().map(ToString::to_string).collect(),
        }
    }
}

/// A breakpoint was set on line &1.
pub const BKP_SET: &str = "CPF9C01";
/// A conditional breakpoint was set on line &1.
pub const BKP_COND: &str = "CPF9C01";
/// A thread breakpoint was set on line &1 for thread &2.
pub const BKP_THREAD: &str = "CPF9C01";
/// The breakpoint on line &1 was removed.
pub const BKP_REMOVED: &str = "CPF9C17";
/// No breakpoint is on line &1.
pub const BKP_NONE_AT: &str = "CPF9C29";
/// Every breakpoint of the program was removed (&1).
pub const BKP_CLEARED: &str = "CPF9C37";
/// The breakpoints of the module are on lines &1.
pub const BKP_LIST: &str = "CPI9BA4";
/// No breakpoints exist.
pub const BKP_NONE: &str = "CPF9C04";
/// No breakpoint in that direction.
pub const BKP_NONE_DIR: &str = "CPF9C04";
/// No equates are defined.
pub const EQU_NONE: &str = "CPF9C38";
/// The equates: &1.
pub const EQU_LIST: &str = "CPI9BA6";
/// &1 is now an equate.
pub const EQU_SET: &str = "CPI9BA7";
/// &1 is no longer an equate.
pub const EQU_GONE: &str = "CPI9BA8";
/// Watch &1 was set on &2.
pub const WATCH_SET: &str = "CPF9C08";
/// &1 is already watched, or no more watches fit.
pub const WATCH_FULL: &str = "CPD9CC3";
/// Watch &1 was removed.
pub const WATCH_REMOVED: &str = "CPF9C10";
/// There is no watch &1.
pub const WATCH_NONE: &str = "CPF9C65";
/// Every watch was removed.
pub const WATCH_CLEARED: &str = "CPF9C12";
/// Variables are looked up at line &1.
pub const QUAL_SET: &str = "CPF9C33";
/// &1 was found on line &2.
pub const FOUND: &str = "CPI9BAC";
/// &1 was not found.
pub const NOT_FOUND: &str = "CPF9C14";
/// FIND matches case now.
pub const CASE_ON: &str = "CPI9BAD";
/// FIND ignores case now.
pub const CASE_OFF: &str = "CPI9BAE";
/// OPM source debug is on.
pub const OPMSRC_ON: &str = "CPI9BAF";
/// OPM source debug is off.
pub const OPMSRC_OFF: &str = "CPI9BBB";
/// Type a debug command.
pub const EMPTY: &str = "CPD9CC6";
/// &1 is not a debug command.
pub const UNKNOWN: &str = "CPF9C43";
/// Operand &1 is not valid here.
pub const OPERAND: &str = "CPF9C59";
/// &1 needs an operand.
pub const MISSING: &str = "CPF9C42";
/// Line &1 is not in the view.
pub const NO_LINE: &str = "CPF9C19";
/// No statement starts on or after line &1.
pub const NO_STMT: &str = "CPD9CCB";
/// The program is not stopped; &1 cannot be done.
pub const NOT_STOPPED: &str = "CPD9CCC";
/// The module's source is not available.
pub const NO_SOURCE: &str = "CPF9C02";
