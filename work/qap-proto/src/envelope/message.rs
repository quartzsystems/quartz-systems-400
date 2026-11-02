// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QS/400 message carried by a reply or event.

use ipc::wire_struct;

/// A message from a message file, its text already substituted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerMessage {
    /// Seven characters (`CPF1107`).
    pub id: String,
    /// 0 to 99.
    pub severity: u8,
    /// The substitution data.
    pub data: Vec<u8>,
    /// First level.
    pub text: String,
    /// Second level: cause and recovery.
    pub help: String,
}

wire_struct!(ServerMessage {
    id,
    severity,
    data,
    text,
    help
});
