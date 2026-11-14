// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The directory server: its instances, properties, suffixes, indexes,
//! schema, access control and publishing, through the administration
//! interface the storage service answers (`dirsrv::admin::DirAdmin`,
//! request `dir_admin`), as the signed-on user: it checks the
//! authority, validates and words the refusal.

pub use dirsrv::admin::{
    Access, AdminChange, AdminError, AdminInfo, AgentKind, AgentStatus, Counters, DirAdmin,
    ErrorKind, Index, InstanceInfo, InstanceState, Properties, PublishAgent, SchemaDef, SchemaKind,
    SchemaPage, SchemaQuery, Suffix, SuffixKind,
};
use qsweb::msg::{Kind, Message};

/// A refusal as the message row the pages show.
#[must_use]
pub fn message(e: &AdminError) -> Message {
    let data: Vec<&str> = e.data.iter().map(String::as_str).collect();
    Message::new(Kind::Error, &e.msgid, &e.text).with_data(&data)
}
