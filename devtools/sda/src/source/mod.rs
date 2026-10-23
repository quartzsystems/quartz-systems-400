// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display-file source member: its lines ([`line`]), the member
//! split into file level, records and elements ([`member`]) and the
//! writer that puts it back ([`write`]).

pub mod line;
pub mod member;
pub mod write;

pub use line::Line;
pub use member::{Element, Kind, Member, Record, RecordType, Usage, errors, parse};
pub use write::{Stamp, write};
