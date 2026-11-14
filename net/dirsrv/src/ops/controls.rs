// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Request controls (docs/DIRSRV.md §7): each operation takes the ones
//! that apply to it. Any other control, or one not built (IBM's
//! `1.3.18.0.2.10.*`), is unavailableCriticalExtension (12) when critical
//! and ignored when not (RFC 4511 §4.1.11). A control of the operation
//! whose value does not decode is protocolError (2).

use ldap::ResultCode;
use ldap::control::{
    self, Assertion, ControlValue, PagedResults, PersistentSearch, PostReadRequest, PreReadRequest,
    ProxiedAuthz, SortRequest,
};
use ldap::filter::Filter;
use ldap::msg::LdapMessage;

use super::error::{OpError, OpResult};
use super::settings::Settings;

/// Which operation the controls came with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Bind,
    Search,
    Compare,
    Add,
    Modify,
    Delete,
    ModDn,
    Extended,
}

/// The controls an operation honours, decoded.
#[derive(Debug, Clone, Default)]
pub(crate) struct Ctl {
    pub manage: bool,
    pub paged: Option<PagedResults>,
    pub sort: Option<(SortRequest, bool)>,
    pub psearch: Option<PersistentSearch>,
    pub proxy: Option<String>,
    pub tree_delete: bool,
    pub assertion: Option<Filter>,
    pub pre_read: Option<Vec<String>>,
    pub post_read: Option<Vec<String>>,
    pub ppolicy: bool,
}

/// The controls QS/400 advertises in `supportedControl`.
pub(crate) fn supported(s: &Settings) -> Vec<&'static str> {
    let mut v = vec![
        control::PAGED_RESULTS,
        control::SORT_REQUEST,
        control::MANAGE_DSA_IT,
        control::PROXIED_AUTHZ,
        control::TREE_DELETE,
        control::ASSERTION,
        control::PRE_READ,
        control::POST_READ,
        control::PASSWORD_POLICY,
    ];
    if s.psearch {
        v.insert(3, control::PERSISTENT_SEARCH);
    }
    v
}

fn allowed(op: Op, oid: &str) -> bool {
    use Op::{Add, Bind, Delete, Extended, ModDn, Modify, Search};
    match oid {
        control::MANAGE_DSA_IT => !matches!(op, Bind | Extended),
        control::PAGED_RESULTS | control::SORT_REQUEST | control::PERSISTENT_SEARCH => op == Search,
        control::PROXIED_AUTHZ | control::ASSERTION => !matches!(op, Bind | Extended),
        control::TREE_DELETE => op == Delete,
        control::PRE_READ => matches!(op, Modify | Delete | ModDn),
        control::POST_READ => matches!(op, Add | Modify | ModDn),
        control::PASSWORD_POLICY => matches!(op, Bind | Add | Modify | Extended),
        _ => false,
    }
}

fn bad(what: &str) -> OpError {
    OpError::new(
        ResultCode::ProtocolError,
        format!("the {what} control is malformed"),
    )
}

/// The controls of `msg` for `op`, as `admin` (or not) may use them.
///
/// # Errors
/// 12 for a critical control the operation does not honour, 2 for one
/// that does not decode.
pub(crate) fn parse(msg: &LdapMessage, op: Op, s: &Settings, admin: bool) -> OpResult<Ctl> {
    let mut c = Ctl::default();
    for k in &msg.controls {
        let mut ok = allowed(op, &k.oid);
        ok &= match k.oid.as_str() {
            control::PERSISTENT_SEARCH => s.psearch,
            control::PAGED_RESULTS => admin || s.paged_non_admin,
            control::SORT_REQUEST => admin || s.sort_non_admin,
            _ => true,
        };
        if !ok {
            if k.critical {
                return Err(OpError::new(
                    ResultCode::UnavailableCriticalExtension,
                    format!("control {} is not available for this operation", k.oid),
                ));
            }
            continue;
        }
        match k.oid.as_str() {
            control::MANAGE_DSA_IT => c.manage = true,
            control::TREE_DELETE => c.tree_delete = true,
            control::PASSWORD_POLICY => c.ppolicy = true,
            control::PAGED_RESULTS => {
                c.paged = Some(PagedResults::from_control(k).map_err(|_| bad("paged results"))?);
            }
            control::SORT_REQUEST => {
                c.sort = Some((
                    SortRequest::from_control(k).map_err(|_| bad("sort"))?,
                    k.critical,
                ));
            }
            control::PERSISTENT_SEARCH => {
                c.psearch =
                    Some(PersistentSearch::from_control(k).map_err(|_| bad("persistent search"))?);
            }
            control::PROXIED_AUTHZ => {
                if !k.critical {
                    return Err(bad("proxied authorization (it must be critical)"));
                }
                c.proxy = Some(
                    ProxiedAuthz::from_control(k)
                        .map_err(|_| bad("proxied authorization"))?
                        .0,
                );
            }
            control::ASSERTION => {
                c.assertion = Some(Assertion::from_control(k).map_err(|_| bad("assertion"))?.0);
            }
            control::PRE_READ => {
                c.pre_read = Some(
                    PreReadRequest::from_control(k)
                        .map_err(|_| bad("pre-read"))?
                        .0,
                );
            }
            control::POST_READ => {
                c.post_read = Some(
                    PostReadRequest::from_control(k)
                        .map_err(|_| bad("post-read"))?
                        .0,
                );
            }
            _ => {}
        }
    }
    Ok(c)
}
