// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Command` (version 2): run and check CL commands, and describe one
//! for prompting. A definition is flattened into [`ParmNode`]s in the
//! prompter's order, `parent` linking elements and qualifiers to their
//! owner, so no type is recursive.

use ipc::{wire_enum, wire_struct};

use crate::envelope::ServerMessage;
use crate::types::{JobId, ObjectName};

/// `RunCommand`'s and `CheckCommand`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandText {
    /// The command string.
    pub command: String,
}

wire_struct!(CommandText { command });

/// `RunCommand`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandReply {
    /// The job it ran in.
    pub job: JobId,
    /// Every message the command sent to its caller, oldest first.
    pub messages: Vec<ServerMessage>,
}

wire_struct!(CommandReply { job, messages });

/// `CheckCommand`'s reply: the check's messages, empty when valid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReply {
    /// The messages.
    pub messages: Vec<ServerMessage>,
}

wire_struct!(CheckReply { messages });

/// `DescribeCommand`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DescribeCommand {
    /// The command; the library may be `*LIBL`.
    pub command: ObjectName,
}

wire_struct!(DescribeCommand { command });

/// What a node is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    /// A parameter, with its keyword.
    #[default]
    Parameter,
    /// An element of a parameter's list.
    Element,
    /// A part of a qualified name.
    Qualifier,
}

wire_enum!(Role {
    0 => Parameter,
    1 => Element,
    2 => Qualifier,
});

/// A value's kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ParmKind {
    /// A name.
    #[default]
    Name,
    /// Characters.
    Character,
    /// A packed decimal.
    Decimal,
    /// A binary integer.
    Integer,
    /// A logical: `'0'` or `'1'`.
    Logical,
    /// A date.
    Date,
    /// A time.
    Time,
    /// A generic name (`ABC*`).
    Generic,
    /// A path name.
    Path,
    /// A command string.
    Command,
}

wire_enum!(ParmKind {
    0 => Name,
    1 => Character,
    2 => Decimal,
    3 => Integer,
    4 => Logical,
    5 => Date,
    6 => Time,
    7 => Generic,
    8 => Path,
    9 => Command,
});

/// When the prompter shows a node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Show {
    /// On the first display.
    #[default]
    Always,
    /// After F10, under Additional Parameters.
    Additional,
    /// Never: a constant.
    Never,
}

wire_enum!(Show {
    0 => Always,
    1 => Additional,
    2 => Never,
});

/// A value range.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Range {
    /// The lowest value.
    pub low: String,
    /// The highest.
    pub high: String,
}

wire_struct!(Range { low, high });

/// One parameter, element or qualifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParmNode {
    /// Its index in `nodes`.
    pub index: u16,
    /// The node it belongs to.
    pub parent: Option<u16>,
    /// What it is.
    pub role: Role,
    /// The keyword; empty for elements and qualifiers.
    pub keyword: String,
    /// The prompt text.
    pub prompt: String,
    /// Help text.
    pub help: String,
    /// The value's kind.
    pub kind: ParmKind,
    /// Its length (digits for decimals).
    pub length: u32,
    /// Decimal positions.
    pub decimals: u16,
    /// A value must be given.
    pub required: bool,
    /// The fewest values.
    pub min_values: u16,
    /// The most; more than 1 is a list.
    pub max_values: u16,
    /// The default.
    pub default: Option<String>,
    /// Special and single values (`*ALL`).
    pub special_values: Vec<String>,
    /// The only values allowed, when restricted.
    pub values: Vec<String>,
    /// The range allowed.
    pub range: Option<Range>,
    /// When the prompter shows it.
    pub show: Show,
}

wire_struct!(ParmNode {
    index,
    parent,
    role,
    keyword,
    prompt,
    help,
    kind,
    length,
    decimals,
    required,
    min_values,
    max_values,
    default,
    special_values,
    values,
    range,
    show
});

/// `DescribeCommand`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandDefinition {
    /// The command as resolved.
    pub name: ObjectName,
    /// Its description.
    pub text: String,
    /// Its help.
    pub help: String,
    /// Its parameters, flattened.
    pub nodes: Vec<ParmNode>,
}

wire_struct!(CommandDefinition {
    name,
    text,
    help,
    nodes
});
