// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Debug` (version 9, draft): the source debugger over QAP. One debug
//! session per connection: the connection job's own calls, or another
//! job attached as `STRSRVJOB` attaches it or by a service entry point.
//! Line numbers are the `*SOURCE` view's (docs/QAP.md, "Service 11
//! `Debug`").

use ipc::{wire_enum, wire_struct};

use crate::envelope::ServerMessage;
use crate::types::{JobId, ObjectName};

/// What a session debugs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum DebugTarget {
    /// The connection job: the programs `RunCommand` calls.
    #[default]
    Own,
    /// Another job of the profile (or any, with `*SERVICE`).
    Job {
        /// The job.
        job: JobId,
    },
}

wire_enum!(DebugTarget {
    0 => Own,
    1 => Job { job },
});

/// `Start`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Start {
    /// What to debug.
    pub target: DebugTarget,
}

wire_struct!(Start { target });

/// `Start`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Started {
    /// The session's number on this connection.
    pub session: u32,
}

wire_struct!(Started { session });

/// A view of a module.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum View {
    /// Statement numbers.
    #[default]
    Statement,
    /// The source member.
    Source,
    /// The compile listing.
    Listing,
}

wire_enum!(View {
    0 => Statement,
    1 => Source,
    2 => Listing,
});

impl View {
    /// Every view, in value order.
    pub const ALL: [View; 3] = [View::Statement, View::Source, View::Listing];
}

/// `AddProgram`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddProgram {
    /// The program; the library may be `*LIBL`.
    pub program: ObjectName,
    /// `*PGM` or `*SRVPGM`.
    pub r#type: String,
}

wire_struct!(AddProgram { program, r#type });

/// A module of a program under debug.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugModule {
    /// The module.
    pub name: String,
    /// The views its debug data offers.
    pub views: Vec<View>,
}

wire_struct!(DebugModule { name, views });

/// `AddProgram`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProgramAdded {
    /// The program's modules, in binding order.
    pub modules: Vec<DebugModule>,
}

wire_struct!(ProgramAdded { modules });

/// A breakpoint asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineBreakpoint {
    /// The `*SOURCE` view's line, 1-based.
    pub line: u32,
    /// A condition, `variable op value`; none for always.
    pub condition: Option<String>,
}

wire_struct!(LineBreakpoint { line, condition });

/// `SetBreakpoints`'s request: the module's breakpoints from now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetBreakpoints {
    /// The program.
    pub program: ObjectName,
    /// The module.
    pub module: String,
    /// The breakpoints; empty removes the module's.
    pub lines: Vec<LineBreakpoint>,
}

wire_struct!(SetBreakpoints {
    program,
    module,
    lines
});

/// A breakpoint as set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakpointSet {
    /// The line it landed on: the first statement at or after the one
    /// asked for.
    pub line: u32,
    /// Its statement.
    pub stmt: u32,
}

wire_struct!(BreakpointSet { line, stmt });

/// `SetBreakpoints`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakpointsSet {
    /// One per breakpoint asked for, in its order.
    pub set: Vec<BreakpointSet>,
}

wire_struct!(BreakpointsSet { set });

/// How a stopped job goes on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResumeHow {
    /// To the next breakpoint.
    #[default]
    Go,
    /// To the next statement of this procedure or its callers.
    StepOver,
    /// To the next statement anywhere, into calls.
    StepInto,
    /// To the next statement of a caller.
    StepOut,
}

wire_enum!(ResumeHow {
    0 => Go,
    1 => StepOver,
    2 => StepInto,
    3 => StepOut,
});

impl ResumeHow {
    /// Every way, in value order.
    pub const ALL: [ResumeHow; 4] = [
        ResumeHow::Go,
        ResumeHow::StepOver,
        ResumeHow::StepInto,
        ResumeHow::StepOut,
    ];
}

/// `Resume`'s request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resume {
    /// How.
    pub how: ResumeHow,
}

wire_struct!(Resume { how });

/// A call stack entry of the stopped job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// The program.
    pub program: ObjectName,
    /// The module.
    pub module: String,
    /// The procedure.
    pub procedure: String,
    /// The statement it is at.
    pub stmt: u32,
    /// That statement's line in `view`; 0 when not known.
    pub line: u32,
    /// The view `line` is of.
    pub view: View,
}

wire_struct!(Frame {
    program,
    module,
    procedure,
    stmt,
    line,
    view
});

/// `Stack`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frames {
    /// The stack, the newest entry first.
    pub frames: Vec<Frame>,
}

wire_struct!(Frames { frames });

/// `Variables`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variables {
    /// The frame, 0 for the newest.
    pub frame: u32,
    /// A structure or array to expand; none for the frame's own.
    pub parent: Option<String>,
}

wire_struct!(Variables { frame, parent });

/// A variable at the stop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variable {
    /// Its name as the source spells it.
    pub name: String,
    /// Its type.
    pub r#type: String,
    /// Its value as `EVAL` shows it.
    pub value: String,
    /// A structure or array with parts.
    pub expandable: bool,
}

wire_struct!(Variable {
    name,
    r#type,
    value,
    expandable
});

/// `Variables`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VariableList {
    /// The variables, in the order the program declares them.
    pub variables: Vec<Variable>,
}

wire_struct!(VariableList { variables });

/// `Eval`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Eval {
    /// The frame, 0 for the newest.
    pub frame: u32,
    /// `NAME`, `NAME:x` or `NAME:c`.
    pub expression: String,
}

wire_struct!(Eval { frame, expression });

/// `Eval`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evaluated {
    /// The value as `EVAL` shows it.
    pub value: String,
}

wire_struct!(Evaluated { value });

/// `SetServiceEntry`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetServiceEntry {
    /// The program.
    pub program: ObjectName,
    /// The user whose jobs it stops: `*CURRENT` or a profile.
    pub user: String,
}

wire_struct!(SetServiceEntry { program, user });

/// `RemoveServiceEntry`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoveServiceEntry {
    /// The program.
    pub program: ObjectName,
}

wire_struct!(RemoveServiceEntry { program });

/// Why a job stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StopReason {
    /// At a breakpoint.
    #[default]
    Breakpoint,
    /// A step ended.
    Step,
    /// A service entry point attached the job.
    ServiceEntry,
    /// An exception the program did not handle.
    Exception {
        /// The exception's message.
        message: ServerMessage,
    },
    /// The program ended while stepping.
    ProgramEnd,
}

wire_enum!(StopReason {
    0 => Breakpoint,
    1 => Step,
    2 => ServiceEntry,
    3 => Exception { message },
    4 => ProgramEnd,
});

/// Event 100 `Stopped`'s body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stopped {
    /// The job that stopped.
    pub job: JobId,
    /// Why.
    pub reason: StopReason,
    /// Its newest frame, as `Stack` gives it.
    pub frame: Frame,
}

wire_struct!(Stopped { job, reason, frame });

/// Event 101 `DebugEnded`'s body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugEnded {
    /// Why, when not by `End`.
    pub message: Option<ServerMessage>,
}

wire_struct!(DebugEnded { message });
