// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LDAPv3` for QS/400 (docs/DIRSRV.md §3), without the store or the job:
//!
//! - [`ber`]: the BER subset LDAP uses.
//! - [`msg`]: `LDAPMessage` and every protocol operation.
//! - [`dn`]: distinguished names (RFC 4514).
//! - [`filter`]: search filters (RFC 4515).
//! - [`control`], [`ext`]: controls and extended operations.
//! - [`ldif`]: LDIF (RFC 2849).
//! - [`url`]: LDAP URLs (RFC 4516).
//! - [`schema`]: schema descriptions (RFC 4512 §4.1).
//! - [`code`]: result codes.
//! - [`client`]: a blocking client.
//!
//! The types here are the interface every other directory crate uses;
//! the codecs fill them.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// Codecs read best with short names for readers, writers and tags.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

pub mod ber;
pub mod client;
pub mod code;
pub mod control;
pub mod dn;
pub mod ext;
pub mod filter;
pub mod ldif;
pub mod msg;
pub mod schema;
pub mod url;

pub use code::ResultCode;
pub use dn::{Ava, Dn, Rdn};
pub use filter::Filter;
pub use msg::{Attribute, LdapMessage, LdapResult, ProtocolOp};
