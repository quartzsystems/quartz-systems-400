// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's messages: the three of `web/messages.html`, one an
//! inquiry waiting for its reply.

use crate::env::msgs::Msgs;

/// A sample message: date and time, severity, message ID, text, from,
/// type.
pub type Msg = [&'static str; 6];

/// The user's messages, newest first.
pub const MESSAGES: &[Msg] = &[
    ["10/29/26 13:58:05", "99", "CPA0702", "CPF0001 received by procedure INVLOAD. (C D I R)", "123503/CWELLMAN/INVBATCH", "Inquiry"],
    ["10/29/26 13:05:10", "00", "CPF1241", "Job 123488/CWELLMAN/QPADEV0003 submitted to job queue QBATCH in QGPL.", "123488/CWELLMAN/QPADEV0003", "Info"],
    ["10/28/26 17:41:30", "80", "", "From OPERATOR: PRT02 is out of labels, holding your INVLABEL file until tomorrow.", "OPERATOR", "Info"],

];

/// The sample messages.
#[derive(Debug, Default)]
pub struct FixtureMsgs;

impl Msgs for FixtureMsgs {}
