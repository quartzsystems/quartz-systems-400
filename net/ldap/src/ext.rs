// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended operations: the OIDs QS/400 knows (docs/DIRSRV.md §7) and
//! their request and response values.

/// `StartTLS` (RFC 4511 §4.14).
pub const START_TLS: &str = "1.3.6.1.4.1.1466.20037";
/// Who am I (RFC 4532).
pub const WHO_AM_I: &str = "1.3.6.1.4.1.4203.1.11.3";
/// Password modify (RFC 3062).
pub const PASSWORD_MODIFY: &str = "1.3.6.1.4.1.4203.1.11.1";
/// Notice of disconnection (RFC 4511 §4.4.1).
pub const NOTICE_OF_DISCONNECTION: &str = "1.3.6.1.4.1.1466.20036";
