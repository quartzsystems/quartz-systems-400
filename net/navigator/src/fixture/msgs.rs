// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample message queues and history log.

use crate::env::msgs::Msgs;

/// The sample system's messages.
#[derive(Default)]
pub struct FixtureMsgs {}

impl Msgs for FixtureMsgs {}
