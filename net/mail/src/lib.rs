// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Outbound mail (docs/SERVERS.md "Mail out"): `SNDSMTPEMM` and the
//! `QtmmSendMail` API hand a message to the smart host `CHGSMTPA`
//! names. There is no SMTP server.
//!
//! - [`addr`]: mailbox addresses.
//! - [`mime`]: the message: headers, a plain or HTML note, attachments.
//! - [`encode`]: base64, quoted-printable, RFC 2047 words, dates.
//! - [`smtp`]: the client dialogue (RFC 5321) with STARTTLS (RFC 3207)
//!   and AUTH PLAIN/LOGIN (RFC 4954).

#![deny(missing_docs)]

pub mod addr;
pub mod encode;
pub mod mime;
pub mod smtp;

pub use addr::Address;
pub use mime::{Attachment, Body, Message};
pub use smtp::{Envelope, Relay, SmtpError, TlsMode};
