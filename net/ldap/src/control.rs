// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Controls (RFC 4511 §4.1.11) and the OIDs of those QS/400 knows
//! (docs/DIRSRV.md §7). The typed values of each are this module's.

/// A control as carried: its type, criticality and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    /// The control's OID.
    pub oid: String,
    /// Whether it is critical.
    pub critical: bool,
    /// Its value, if any.
    pub value: Option<Vec<u8>>,
}

/// Simple paged results (RFC 2696).
pub const PAGED_RESULTS: &str = "1.2.840.113556.1.4.319";
/// Server-side sort request (RFC 2891).
pub const SORT_REQUEST: &str = "1.2.840.113556.1.4.473";
/// Server-side sort response (RFC 2891).
pub const SORT_RESPONSE: &str = "1.2.840.113556.1.4.474";
/// manageDsaIT (RFC 3296).
pub const MANAGE_DSA_IT: &str = "2.16.840.1.113730.3.4.2";
/// Persistent search.
pub const PERSISTENT_SEARCH: &str = "2.16.840.1.113730.3.4.3";
/// Entry change notification (persistent search's response).
pub const ENTRY_CHANGE: &str = "2.16.840.1.113730.3.4.7";
/// Proxied authorization v2 (RFC 4370).
pub const PROXIED_AUTHZ: &str = "2.16.840.1.113730.3.4.18";
/// Tree delete.
pub const TREE_DELETE: &str = "1.2.840.113556.1.4.805";
/// Assertion (RFC 4528).
pub const ASSERTION: &str = "1.3.6.1.1.12";
/// Pre-read (RFC 4527).
pub const PRE_READ: &str = "1.3.6.1.1.13.1";
/// Post-read (RFC 4527).
pub const POST_READ: &str = "1.3.6.1.1.13.2";
/// Password policy (draft-behera).
pub const PASSWORD_POLICY: &str = "1.3.6.1.4.1.42.2.27.8.5.1";
