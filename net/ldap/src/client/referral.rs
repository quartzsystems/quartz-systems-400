// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Following referrals and continuation references (RFC 4511 §4.1.10,
//! §4.5.3), when [`super::Options::referrals`] asks for it. Each hop
//! connects to the URL's server and binds with what the rebind hook
//! gives (anonymously without one), then repeats the operation there.

use super::ops::SearchResult;
use super::{Client, ClientError, Response, Result};
use crate::control::Control;
use crate::msg::{ProtocolOp, SearchRequest};
use crate::url::LdapUrl;

/// The rebind hook: asked, for each referred server, for the DN and
/// password to bind with; none binds anonymously.
pub type Rebind = Box<dyn FnMut(&LdapUrl) -> Option<(String, Vec<u8>)> + Send>;

/// `op` aimed at `dn` instead, where the operation names an entry.
fn retarget(op: &ProtocolOp, dn: &str) -> ProtocolOp {
    let mut op = op.clone();
    match &mut op {
        ProtocolOp::AddRequest { dn: d, .. }
        | ProtocolOp::ModifyRequest { dn: d, .. }
        | ProtocolOp::ModDnRequest { dn: d, .. }
        | ProtocolOp::CompareRequest { dn: d, .. }
        | ProtocolOp::DelRequest(d) => *d = dn.to_string(),
        ProtocolOp::SearchRequest(s) => s.base = dn.to_string(),
        _ => {}
    }
    op
}

impl Client {
    /// Runs `f` against the server of `url`, bound as the rebind hook
    /// says, one hop further.
    fn with_referred<T>(
        &mut self,
        url: &LdapUrl,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        if self.hops >= self.options.hop_limit {
            return Err(ClientError::HopLimit);
        }
        let mut child = Self::connect_url(url, self.options.clone())?;
        child.hops = self.hops + 1;
        let creds = self.rebind.as_mut().and_then(|h| h(url));
        child.rebind = self.rebind.take();
        let out = (|| {
            if let Some((dn, pw)) = creds {
                child.simple_bind(&dn, &pw)?.success()?;
            }
            f(&mut child)
        })();
        self.rebind = child.rebind.take();
        let _ = child.unbind();
        out
    }

    /// Follows a referral result for `op`: the first URL whose server
    /// answers gives the response. None if no URL could be used.
    pub(super) fn chase_op(
        &mut self,
        op: &ProtocolOp,
        controls: &[Control],
        urls: &[String],
    ) -> Result<Option<Response>> {
        for u in urls {
            let Ok(url) = LdapUrl::parse(u) else { continue };
            let target = if url.dn.is_empty() {
                op.clone()
            } else {
                retarget(op, &url.dn)
            };
            match self.with_referred(&url, |c| c.request(&target, controls)) {
                Ok(r) => return Ok(Some(r)),
                Err(ClientError::HopLimit) => return Err(ClientError::HopLimit),
                Err(e) => tracing::debug!(url = %u, error = %e, "ldap referral not followed"),
            }
        }
        Ok(None)
    }

    /// Follows the references of `out` and a referral result, merging
    /// what the referred servers return. References that could not be
    /// followed stay in `out.references`; a referral result that could
    /// not be followed stays the result.
    pub(super) fn chase_search(
        &mut self,
        req: &SearchRequest,
        controls: &[Control],
        out: &mut SearchResult,
    ) -> Result<()> {
        let pending: Vec<Vec<String>> = std::mem::take(&mut out.references);
        for urls in pending {
            if !self.chase_one(req, controls, &urls, out)? {
                out.references.push(urls);
            }
        }
        if out.result.code == crate::code::ResultCode::Referral {
            let held = out.result.clone();
            out.result = crate::msg::LdapResult::success();
            if !self.chase_one(req, controls, &held.referral, out)? {
                out.result = held;
            }
        }
        Ok(())
    }

    /// Searches the first usable server of `urls` into `out`; false if
    /// none could be used.
    fn chase_one(
        &mut self,
        req: &SearchRequest,
        controls: &[Control],
        urls: &[String],
        out: &mut SearchResult,
    ) -> Result<bool> {
        for u in urls {
            let Ok(url) = LdapUrl::parse(u) else { continue };
            let mut sub = req.clone();
            if !url.dn.is_empty() {
                sub.base.clone_from(&url.dn);
            }
            if let Some(s) = url.scope {
                sub.scope = s;
            }
            if url.filter.is_some() {
                match url.filter() {
                    Ok(f) => sub.filter = f,
                    Err(_) => continue,
                }
            }
            match self.with_referred(&url, |c| c.search(&sub, controls)) {
                Ok(r) => {
                    out.entries.extend(r.entries);
                    out.references.extend(r.references);
                    out.intermediates.extend(r.intermediates);
                    if !r.result.is_success() && out.result.is_success() {
                        out.result = r.result;
                    }
                    return Ok(true);
                }
                Err(ClientError::HopLimit) => return Err(ClientError::HopLimit),
                Err(e) => tracing::debug!(url = %u, error = %e, "ldap reference not followed"),
            }
        }
        Ok(false)
    }
}
