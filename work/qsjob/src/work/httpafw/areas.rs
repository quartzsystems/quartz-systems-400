// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The areas whose pages are not built yet: their routes answer the Not
//! available page (`XWA0099`), so their traits ask nothing of the
//! system. Each area's agent replaces its part with a `Live*` of its own.

use accessweb::env::{cmd, download, jobs, msgs};

/// No area's data yet.
// BLOCKED(rel): messages, jobs, commands and download are other agents' steps.
pub struct NotYet;

impl msgs::Msgs for NotYet {}
impl jobs::Jobs for NotYet {}
impl cmd::Cmd for NotYet {}
impl download::Download for NotYet {}
