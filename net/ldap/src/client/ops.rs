// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The synchronous operations, and the paging and sorting helpers.
//! Each returns the server's result whatever its code; only a broken
//! exchange is an error.

use std::time::{Duration, Instant};

use super::{Client, ClientError, Result};
use crate::code::ResultCode;
use crate::control::{Control, ControlValue, PagedResults, SortKey, SortRequest, SortResponse};
use crate::ext::{self, PasswordModifyRequest, PasswordModifyResponse};
use crate::filter::Filter;
use crate::msg::{Attribute, BindAuth, LdapResult, Modification, ProtocolOp, Scope, SearchRequest};

/// A response: the result and the controls that came with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// The result.
    pub result: LdapResult,
    /// The response's controls.
    pub controls: Vec<Control>,
}

impl Response {
    /// The response if it is a success, else the result as an error.
    ///
    /// # Errors
    ///
    /// [`ClientError::Result`].
    pub fn success(self) -> Result<Self> {
        if self.result.is_success() {
            Ok(self)
        } else {
            Err(ClientError::Result(self.result))
        }
    }
}

/// One entry of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEntry {
    /// The DN.
    pub dn: String,
    /// The attributes.
    pub attrs: Vec<Attribute>,
    /// The entry's controls (entry change notification and the like).
    pub controls: Vec<Control>,
}

impl SearchEntry {
    /// The values of `ty` (ASCII case ignored).
    #[must_use]
    pub fn values(&self, ty: &str) -> Option<&[Vec<u8>]> {
        self.attrs
            .iter()
            .find(|a| a.ty.eq_ignore_ascii_case(ty))
            .map(|a| a.vals.as_slice())
    }

    /// The first value of `ty` as text.
    #[must_use]
    pub fn first(&self, ty: &str) -> Option<&str> {
        self.values(ty)?
            .first()
            .and_then(|v| std::str::from_utf8(v).ok())
    }
}

/// Everything a search returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// The entries, in order.
    pub entries: Vec<SearchEntry>,
    /// The continuation references (each a list of URLs) not followed.
    pub references: Vec<Vec<String>>,
    /// Intermediate responses: name and value.
    pub intermediates: Vec<(Option<String>, Option<Vec<u8>>)>,
    /// The final result.
    pub result: LdapResult,
    /// The final result's controls.
    pub controls: Vec<Control>,
}

impl Client {
    fn deadline(&self) -> Option<Instant> {
        self.options.timeout.map(|t| Instant::now() + t)
    }

    fn wait_until(
        &mut self,
        id: i32,
        deadline: Option<Instant>,
    ) -> Result<crate::msg::LdapMessage> {
        let left = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        if left.is_some_and(|l| l.is_zero()) {
            let _ = self.abandon(id);
            return Err(ClientError::Timeout);
        }
        if let Some(m) = self.poll(Some(id), left)? {
            Ok(m)
        } else {
            let _ = self.abandon(id);
            Err(ClientError::Timeout)
        }
    }

    /// Sends `op` and waits for its one response.
    fn exchange(
        &mut self,
        op: ProtocolOp,
        controls: Vec<Control>,
    ) -> Result<(ProtocolOp, Vec<Control>)> {
        let deadline = self.deadline();
        let id = self.send(op, controls)?;
        loop {
            let m = self.wait_until(id, deadline)?;
            if matches!(m.op, ProtocolOp::IntermediateResponse { .. }) {
                continue;
            }
            return Ok((m.op, m.controls));
        }
    }

    /// Runs an operation with a single result (add, modify, delete,
    /// modify DN, compare), chasing a referral if [`super::Options::referrals`]
    /// is set.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn request(&mut self, op: &ProtocolOp, controls: &[Control]) -> Result<Response> {
        let (resp, ctl) = self.exchange(op.clone(), controls.to_vec())?;
        let result = resp
            .result()
            .cloned()
            .ok_or(ClientError::Unexpected("the response carries no result"))?;
        if result.code == ResultCode::Referral
            && self.options.referrals
            && let Some(r) = self.chase_op(op, controls, &result.referral)?
        {
            return Ok(r);
        }
        Ok(Response {
            result,
            controls: ctl,
        })
    }

    /// A simple bind (an empty password binds anonymously or
    /// unauthenticated).
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn simple_bind(&mut self, dn: &str, password: &[u8]) -> Result<Response> {
        let (r, _) = self.bind(dn, BindAuth::Simple(password.to_vec()), &[])?;
        Ok(r)
    }

    /// A SASL bind step: the response and the server's credentials.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn sasl_bind(
        &mut self,
        dn: &str,
        mechanism: &str,
        credentials: Option<&[u8]>,
        controls: &[Control],
    ) -> Result<(Response, Option<Vec<u8>>)> {
        self.bind(
            dn,
            BindAuth::Sasl {
                mechanism: mechanism.to_string(),
                credentials: credentials.map(<[u8]>::to_vec),
            },
            controls,
        )
    }

    fn bind(
        &mut self,
        dn: &str,
        auth: BindAuth,
        controls: &[Control],
    ) -> Result<(Response, Option<Vec<u8>>)> {
        let op = ProtocolOp::BindRequest {
            version: self.options.version,
            name: dn.to_string(),
            auth,
        };
        match self.exchange(op, controls.to_vec())? {
            (
                ProtocolOp::BindResponse {
                    result,
                    server_sasl_creds,
                },
                controls,
            ) => Ok((Response { result, controls }, server_sasl_creds)),
            _ => Err(ClientError::Unexpected("a bind needs a bind response")),
        }
    }

    /// A search request of `base`, `scope` and the filter string, for the
    /// attributes `attrs`, with the options' alias dereferencing.
    ///
    /// # Errors
    ///
    /// A bad filter ([`ClientError::Unexpected`]).
    pub fn search_request(
        &self,
        base: &str,
        scope: Scope,
        filter: &str,
        attrs: &[&str],
    ) -> Result<SearchRequest> {
        let filter =
            Filter::parse(filter).map_err(|_| ClientError::Unexpected("bad search filter"))?;
        let mut req = SearchRequest::new(base.to_string(), scope, filter);
        req.deref = self.options.deref;
        req.attrs = attrs.iter().map(|a| (*a).to_string()).collect();
        Ok(req)
    }

    /// Searches, collecting every entry and reference until the result;
    /// zero limits in `req` take the options' limits. References and a
    /// referral result are followed if [`super::Options::referrals`] is
    /// set.
    ///
    /// # Errors
    ///
    /// A broken exchange, or [`ClientError::Timeout`] (the search is then
    /// abandoned).
    pub fn search(&mut self, req: &SearchRequest, controls: &[Control]) -> Result<SearchResult> {
        let mut req = req.clone();
        if req.size_limit == 0 {
            req.size_limit = self.options.size_limit;
        }
        if req.time_limit == 0 {
            req.time_limit = self.options.time_limit;
        }
        let deadline = self.deadline();
        let id = self.send(ProtocolOp::SearchRequest(req.clone()), controls.to_vec())?;
        let mut out = SearchResult {
            entries: Vec::new(),
            references: Vec::new(),
            intermediates: Vec::new(),
            result: LdapResult::success(),
            controls: Vec::new(),
        };
        loop {
            let m = self.wait_until(id, deadline)?;
            match m.op {
                ProtocolOp::SearchResultEntry { dn, attrs } => out.entries.push(SearchEntry {
                    dn,
                    attrs,
                    controls: m.controls,
                }),
                ProtocolOp::SearchResultReference(urls) => out.references.push(urls),
                ProtocolOp::IntermediateResponse { name, value } => {
                    out.intermediates.push((name, value));
                }
                ProtocolOp::SearchResultDone(r) => {
                    out.result = r;
                    out.controls = m.controls;
                    break;
                }
                _ => return Err(ClientError::Unexpected("not a search response")),
            }
        }
        if self.options.referrals {
            self.chase_search(&req, controls, &mut out)?;
        }
        Ok(out)
    }

    /// One page of a paged search (RFC 2696): the page and the cookie for
    /// the next one (empty after the last).
    ///
    /// # Errors
    ///
    /// A broken exchange, or a bad paged results response.
    pub fn search_page(
        &mut self,
        req: &SearchRequest,
        size: i32,
        cookie: &[u8],
        controls: &[Control],
    ) -> Result<(SearchResult, Vec<u8>)> {
        let mut controls: Vec<Control> = controls
            .iter()
            .filter(|c| c.oid != PagedResults::OID)
            .cloned()
            .collect();
        controls.push(
            PagedResults {
                size,
                cookie: cookie.to_vec(),
            }
            .to_control(true),
        );
        let r = self.search(req, &controls)?;
        let next = PagedResults::find(&r.controls)
            .map_err(|_| ClientError::Unexpected("bad paged results response"))?
            .map(|p| p.cookie)
            .unwrap_or_default();
        Ok((r, next))
    }

    /// A whole search in pages of `size`: the entries of every page and
    /// the last page's result. Stops at the first result that is not a
    /// success.
    ///
    /// # Errors
    ///
    /// What [`Client::search_page`] returns.
    pub fn search_paged(
        &mut self,
        req: &SearchRequest,
        size: i32,
        controls: &[Control],
    ) -> Result<SearchResult> {
        let mut cookie = Vec::new();
        let mut all: Option<SearchResult> = None;
        loop {
            let (page, next) = self.search_page(req, size, &cookie, controls)?;
            let done = next.is_empty() || !page.result.is_success();
            all = Some(match all {
                None => page,
                Some(mut a) => {
                    a.entries.extend(page.entries);
                    a.references.extend(page.references);
                    a.intermediates.extend(page.intermediates);
                    a.result = page.result;
                    a.controls = page.controls;
                    a
                }
            });
            if done {
                return all.ok_or(ClientError::Unexpected("no page"));
            }
            cookie = next;
        }
    }

    /// A search sorted by the server (RFC 2891): the result and the sort
    /// response, if the server sent one.
    ///
    /// # Errors
    ///
    /// A broken exchange, or a bad sort response.
    pub fn search_sorted(
        &mut self,
        req: &SearchRequest,
        keys: Vec<SortKey>,
        critical: bool,
        controls: &[Control],
    ) -> Result<(SearchResult, Option<SortResponse>)> {
        let mut controls: Vec<Control> = controls
            .iter()
            .filter(|c| c.oid != SortRequest::OID)
            .cloned()
            .collect();
        controls.push(SortRequest(keys).to_control(critical));
        let r = self.search(req, &controls)?;
        let s = SortResponse::find(&r.controls)
            .map_err(|_| ClientError::Unexpected("bad sort response"))?;
        Ok((r, s))
    }

    /// Adds an entry.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn add(
        &mut self,
        dn: &str,
        attrs: Vec<Attribute>,
        controls: &[Control],
    ) -> Result<Response> {
        self.request(
            &ProtocolOp::AddRequest {
                dn: dn.to_string(),
                attrs,
            },
            controls,
        )
    }

    /// Modifies an entry.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn modify(
        &mut self,
        dn: &str,
        changes: Vec<Modification>,
        controls: &[Control],
    ) -> Result<Response> {
        self.request(
            &ProtocolOp::ModifyRequest {
                dn: dn.to_string(),
                changes,
            },
            controls,
        )
    }

    /// Deletes an entry.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn delete(&mut self, dn: &str, controls: &[Control]) -> Result<Response> {
        self.request(&ProtocolOp::DelRequest(dn.to_string()), controls)
    }

    /// Renames or moves an entry.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn modify_dn(
        &mut self,
        dn: &str,
        new_rdn: &str,
        delete_old_rdn: bool,
        new_superior: Option<&str>,
        controls: &[Control],
    ) -> Result<Response> {
        self.request(
            &ProtocolOp::ModDnRequest {
                dn: dn.to_string(),
                new_rdn: new_rdn.to_string(),
                delete_old_rdn,
                new_superior: new_superior.map(str::to_string),
            },
            controls,
        )
    }

    /// Compares a value; the result is `compareTrue`, `compareFalse` or
    /// an error code.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn compare(
        &mut self,
        dn: &str,
        attr: &str,
        value: &[u8],
        controls: &[Control],
    ) -> Result<Response> {
        self.request(
            &ProtocolOp::CompareRequest {
                dn: dn.to_string(),
                attr: attr.to_string(),
                value: value.to_vec(),
            },
            controls,
        )
    }

    /// An extended operation: the response, its name and its value.
    ///
    /// # Errors
    ///
    /// A broken exchange.
    pub fn extended(
        &mut self,
        name: &str,
        value: Option<Vec<u8>>,
        controls: &[Control],
    ) -> Result<(Response, Option<String>, Option<Vec<u8>>)> {
        let op = ProtocolOp::ExtendedRequest {
            name: name.to_string(),
            value,
        };
        match self.exchange(op, controls.to_vec())? {
            (
                ProtocolOp::ExtendedResponse {
                    result,
                    name,
                    value,
                },
                controls,
            ) => Ok((Response { result, controls }, name, value)),
            _ => Err(ClientError::Unexpected(
                "an extended request needs an extended response",
            )),
        }
    }

    /// Who am I (RFC 4532): the authorization identity, empty when
    /// anonymous.
    ///
    /// # Errors
    ///
    /// [`ClientError::Result`] if the server refuses.
    pub fn who_am_i(&mut self) -> Result<String> {
        let (r, _, v) = self.extended(ext::WHO_AM_I, None, &[])?;
        r.success()?;
        ext::who_am_i_value(v.as_deref()).map_err(|_| ClientError::Unexpected("bad who am I value"))
    }

    /// Password modify (RFC 3062): the generated password, if any.
    ///
    /// # Errors
    ///
    /// [`ClientError::Result`] if the server refuses.
    pub fn password_modify(
        &mut self,
        req: &PasswordModifyRequest,
    ) -> Result<PasswordModifyResponse> {
        let (r, _, v) = self.extended(ext::PASSWORD_MODIFY, Some(req.encode_value()), &[])?;
        r.success()?;
        PasswordModifyResponse::decode_value(v.as_deref())
            .map_err(|_| ClientError::Unexpected("bad password modify response"))
    }

    /// Sets the time a synchronous operation waits.
    pub fn set_timeout(&mut self, t: Option<Duration>) {
        self.options.timeout = t;
    }
}
