// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Publishing users and printers to a directory (docs/DIRSRV.md §10.1):
//! the configuration file ([`config`]), what a profile or a printer is as
//! an entry ([`map`]), where objects and changes come from ([`source`]),
//! the publisher that keeps a target in step ([`publisher`]) and a job's
//! turn ([`runner`]).

pub mod config;
pub mod map;
pub mod publisher;
pub mod runner;
pub mod source;

pub use config::{AgentConfig, Kind, PublishConfig, Security};
pub use map::{Object, PrinterInfo, UserInfo};
pub use publisher::{Connect, Publisher, Status, TcpConnect};
pub use runner::Runner;
pub use source::{
    Change, Changes, Cursor, Listing, MemSource, PublishSource, SourceConfig, Tracker,
};
