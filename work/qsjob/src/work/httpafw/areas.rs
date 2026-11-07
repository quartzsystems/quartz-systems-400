// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The areas whose pages are not built yet: their routes answer the Not
//! available page (`XWA0099`), so their traits ask nothing of the
//! system. Each area's agent replaces its part with a `Live*` of its own.

use accessweb::env::{cmd, download, files, jobs, msgs, print};

/// No area's data yet.
// BLOCKED(rel): files, print, messages, jobs, commands and download are other agents' steps.
pub struct NotYet;

impl files::Files for NotYet {}
impl print::Print for NotYet {}
impl msgs::Msgs for NotYet {}
impl jobs::Jobs for NotYet {}
impl cmd::Cmd for NotYet {}
impl download::Download for NotYet {}
