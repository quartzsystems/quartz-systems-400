// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Searches as resumable cursors (`dir_search`, `dir_next`, §4).
//!
//! [`Directory::search`] checks the request and keeps a cursor in the
//! connection's [`Cursors`]; [`Directory::next`] answers at most a
//! [`Budget`] of entries and bytes per call, so the single-threaded
//! service never runs one long search. A paged search's cursor waits in
//! [`Cursors`] for the next page's request (its cookie is the cursor's
//! number); a persistent search's becomes a watch (`watch`).
//!
//! What follows IBM's server (`ldaptest-primary.txt`): a size limit is
//! exceeded (4) only when more entries match than it allows; the root
//! DSE answers base searches, a one-level search of `""` is 53 and a
//! subtree search of `""` searches every suffix of the entry store;
//! `cn=schema` one-level is 2, `cn=monitor` one-level is 32 and below
//! it 53; a filter naming the ACL attributes is 53 for anyone but an
//! administrator; a paged search past `ibm-slapdPagedResLmt` open at once
//! is 53, as is a cookie the server did not give; a page size of 0 with
//! a cookie ends the search. Sorting (RFC 2891): more keys than
//! `ibm-slapdSortKeyLimit` is sortResult 11, an unknown attribute 16 —
//! the search fails with 12 when the control is critical and is answered
//! unsorted otherwise.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

use ldap::ResultCode;
use ldap::control::{Control, ControlValue, PagedResults, PersistentSearch, SortResponse};
use ldap::dn::Dn;
use ldap::filter::Filter;
use ldap::msg::{Attribute, DerefAliases, LdapMessage, LdapResult, ProtocolOp, Scope, SearchRequest};
use store::txn::Txn;

use super::attrs::{COMPUTED, Wanted};
use super::controls::{self, Op};
use super::filter::{self, Target};
use super::{AclCache, AuditOp, Directory, OpError, OpResult, Place, Reply, Session};
use crate::acl;
use crate::backend::{DirEntry, Scan};
use crate::schema::check::split_description;
use crate::schema::rule::{self, Rule};
use crate::schema::AttributeType;

/// How much one [`Directory::next`] may answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Entries (and references).
    pub entries: usize,
    /// Bytes of entries, roughly as encoded; at least one entry is
    /// answered.
    pub bytes: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            entries: 100,
            bytes: 256 * 1024,
        }
    }
}

/// Where a cursor stands after a [`Directory::next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// More to come: call again.
    More,
    /// The search is over (its `SearchResultDone` is the last reply).
    Done,
    /// A page is over; the cursor waits for the next page's request.
    Paused,
    /// The initial results of a persistent search are over; its changes
    /// come from [`Directory::changes`] under this watch.
    Watching(u64),
}

/// One [`Directory::next`]'s answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Entries, references and at the end, unless more come, the done.
    pub replies: Vec<Reply>,
    /// What next.
    pub state: PageState,
}

/// How a search starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Started {
    /// Answered already (an error, an abandoned page): these replies,
    /// the last a `SearchResultDone`.
    Done(Vec<Reply>),
    /// A cursor: call [`Directory::next`] with it.
    Cursor(u64),
}

struct SortKey {
    oid: String,
    rule: Option<&'static Rule>,
    eq: Option<&'static Rule>,
    reverse: bool,
}

/// A search in progress.
pub(crate) struct Cursor {
    session: Session,
    filter: Filter,
    scan_filter: Filter,
    want: Wanted,
    types_only: bool,
    manage: bool,
    deref_search: bool,
    size_limit: usize,
    deadline: Option<u64>,
    regions: VecDeque<(usize, u64, Scope)>,
    region: Option<(usize, u64, Scope)>,
    scan: Scan,
    pending: VecDeque<u64>,
    fixed: VecDeque<Reply>,
    seen: HashSet<(usize, u64)>,
    sent: usize,
    sort: Option<Vec<SortKey>>,
    sorted: Option<VecDeque<Reply>>,
    sort_ctl: Option<Control>,
    page_size: Option<usize>,
    page_sent: usize,
    paused: bool,
    psearch: Option<(PersistentSearch, Dn, Scope)>,
    cache: AclCache,
}

/// A connection's cursors.
#[derive(Default)]
pub struct Cursors {
    next: u64,
    map: HashMap<u64, Cursor>,
}

impl Cursors {
    /// Open cursors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether none is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// End a cursor (abandon, the connection's end).
    pub fn close(&mut self, id: u64) {
        self.map.remove(&id);
    }

    fn paused_pages(&self) -> usize {
        self.map.values().filter(|c| c.paused).count()
    }
}

fn done(result: LdapResult, controls: Vec<Control>) -> Reply {
    Reply {
        op: ProtocolOp::SearchResultDone(result),
        controls,
    }
}

fn fail(e: &OpError) -> Started {
    Started::Done(vec![done(e.result(), Vec::new())])
}

fn size_of(dn: &str, attrs: &[Attribute]) -> usize {
    dn.len()
        + 16
        + attrs
            .iter()
            .map(|a| a.ty.len() + 8 + a.vals.iter().map(|v| v.len() + 4).sum::<usize>())
            .sum::<usize>()
}

fn deref_bits(d: DerefAliases) -> u8 {
    match d {
        DerefAliases::Never => 0,
        DerefAliases::InSearching => 2,
        DerefAliases::FindingBase => 1,
        DerefAliases::Always => 3,
    }
}

const ACL_ATTRS: [&str; 7] = [
    "aclEntry",
    "aclPropagate",
    "aclSource",
    "entryOwner",
    "ownerPropagate",
    "ownerSource",
    "ibm-effectiveAcl",
];

impl Directory {
    /// The session an operation runs as: the bound one, or the proxied
    /// authorization control's identity (administrators only; anyone
    /// else is 50, as IBM's server answers).
    pub(crate) fn acting(
        &self,
        txn: &mut Txn<'_>,
        session: &Session,
        proxy: Option<&str>,
    ) -> OpResult<Session> {
        let Some(p) = proxy else {
            return Ok(session.clone());
        };
        if !session.admin {
            return Err(OpError::access());
        }
        if p.is_empty() {
            return Ok(Session::anonymous());
        }
        let Some(d) = p.strip_prefix("dn:") else {
            return Err(OpError::new(
                super::error::AUTHORIZATION_DENIED,
                "only dn: authorization identities are taken",
            ));
        };
        let dn = Dn::parse(d).map_err(|_| {
            OpError::new(super::error::AUTHORIZATION_DENIED, "the identity is not a DN")
        })?;
        if self.is_admin_dn(&dn) {
            return Ok(Session::admin(&self.schema, &dn));
        }
        Ok(self.session_for(txn, &dn, super::Method::Simple, &[])?.0)
    }

    /// `dir_search`: start the search `msg` asks for.
    pub fn search(
        &mut self,
        txn: &mut Txn<'_>,
        session: &Session,
        msg: &LdapMessage,
        cursors: &mut Cursors,
    ) -> Started {
        self.counters.ops_initiated += 1;
        self.counters.searches_requested += 1;
        let ProtocolOp::SearchRequest(req) = &msg.op else {
            return fail(&OpError::new(ResultCode::ProtocolError, "not a search request"));
        };
        let r = self.start(txn, session, msg, req, cursors);
        let code = match &r {
            Started::Done(v) => v
                .last()
                .and_then(|r| r.op.result())
                .map_or(ResultCode::Success, |r| r.code),
            Started::Cursor(_) => ResultCode::Success,
        };
        if let Started::Done(_) = r {
            self.counters.searches_completed += 1;
            self.counters.ops_completed += 1;
        }
        self.audit(AuditOp::Search, &req.base, session, code);
        r
    }

    #[allow(clippy::too_many_lines)]
    fn start(
        &mut self,
        txn: &mut Txn<'_>,
        session: &Session,
        msg: &LdapMessage,
        req: &SearchRequest,
        cursors: &mut Cursors,
    ) -> Started {
        let ctl = match controls::parse(msg, Op::Search, &self.settings, session.admin) {
            Ok(c) => c,
            Err(e) => return fail(&e),
        };
        // The next page of a paged search, or its end.
        if let Some(p) = &ctl.paged
            && !p.cookie.is_empty()
        {
            let id = <[u8; 8]>::try_from(p.cookie.as_slice()).map(u64::from_be_bytes);
            let Some(c) = id.ok().and_then(|i| cursors.map.get_mut(&i).map(|c| (i, c))) else {
                return fail(&OpError::unwilling("the paged results cookie is not valid"));
            };
            let (id, c) = c;
            if p.size <= 0 {
                cursors.map.remove(&id);
                let ctl = PagedResults {
                    size: 0,
                    cookie: Vec::new(),
                }
                .to_control(false);
                return Started::Done(vec![done(LdapResult::success(), vec![ctl])]);
            }
            c.page_size = Some(usize::try_from(p.size).unwrap_or(usize::MAX));
            c.page_sent = 0;
            c.paused = false;
            return Started::Cursor(id);
        }
        if let Some(p) = &ctl.paged {
            if p.size <= 0 {
                let ctl = PagedResults::default().to_control(false);
                return Started::Done(vec![done(LdapResult::success(), vec![ctl])]);
            }
            if cursors.paused_pages() >= self.settings.paged_limit as usize {
                return fail(&OpError::unwilling(
                    "too many paged searches are open on this connection",
                ));
            }
        }
        let acting = match self.acting(txn, session, ctl.proxy.as_deref()) {
            Ok(s) => s,
            Err(e) => return fail(&e),
        };
        let base = match Self::parse_dn(&req.base) {
            Ok(d) => d,
            Err(e) => return fail(&e),
        };
        if !acting.admin {
            let mut names = Vec::new();
            filter::names(&req.filter, &mut names);
            let acl_named = names.iter().any(|n| {
                let ty = split_description(n).0;
                ACL_ATTRS.iter().any(|a| {
                    self.schema
                        .attribute(ty)
                        .is_some_and(|at| at.is_named(a))
                })
            });
            if acl_named {
                return fail(&OpError::unwilling(
                    "only an administrator may search by the access control attributes",
                ));
            }
        }
        let now = self.now();
        let admin = acting.admin;
        let lim = |asked: i32, server: u32| -> usize {
            let a = usize::try_from(asked.max(0)).unwrap_or(0);
            let s = server as usize;
            if admin || s == 0 {
                a
            } else if a == 0 {
                s
            } else {
                a.min(s)
            }
        };
        let size_limit = lim(req.size_limit, self.settings.size_limit);
        let time_limit = lim(req.time_limit, self.settings.time_limit);
        let deref = deref_bits(req.deref) & deref_bits(self.settings.deref);
        let mut c = Cursor {
            session: acting.clone(),
            filter: req.filter.clone(),
            scan_filter: if deref & 2 != 0 {
                Filter::Or(vec![
                    req.filter.clone(),
                    Filter::Equality("objectClass".into(), b"alias".to_vec()),
                ])
            } else {
                req.filter.clone()
            },
            want: Wanted::from_list(&req.attrs),
            types_only: req.types_only,
            manage: ctl.manage,
            deref_search: deref & 2 != 0,
            size_limit,
            deadline: (time_limit > 0).then(|| now + time_limit as u64 * 1_000_000),
            regions: VecDeque::new(),
            region: None,
            scan: Scan::Start,
            pending: VecDeque::new(),
            fixed: VecDeque::new(),
            seen: HashSet::new(),
            sent: 0,
            sort: None,
            sorted: None,
            sort_ctl: None,
            page_size: ctl.paged.as_ref().map(|p| usize::try_from(p.size).unwrap_or(usize::MAX)),
            page_sent: 0,
            paused: false,
            psearch: None,
            cache: AclCache::default(),
        };
        // Sorting.
        if let Some((keys, critical)) = &ctl.sort {
            let mut out = Vec::new();
            let mut problem = None;
            if keys.0.len() > self.settings.sort_key_limit as usize {
                problem = Some(ResultCode::AdminLimitExceeded);
            }
            for k in &keys.0 {
                let Some(at) = self.schema.attribute(split_description(&k.attr).0) else {
                    problem.get_or_insert(ResultCode::NoSuchAttribute);
                    continue;
                };
                let rule = match &k.rule {
                    Some(r) => match rule::find(r) {
                        Some(x) => Some(x),
                        None => {
                            problem.get_or_insert(ResultCode::InappropriateMatching);
                            continue;
                        }
                    },
                    None => at.ordering,
                };
                out.push(SortKey {
                    oid: at.oid().to_string(),
                    rule,
                    eq: at.equality,
                    reverse: k.reverse,
                });
            }
            match problem {
                Some(p) => {
                    let sc = SortResponse {
                        result: p,
                        attr: None,
                    }
                    .to_control(false);
                    if *critical {
                        return Started::Done(vec![done(
                            Self::result(
                                ResultCode::UnavailableCriticalExtension,
                                "the requested sort cannot be done",
                            ),
                            vec![sc],
                        )]);
                    }
                    c.sort_ctl = Some(sc);
                }
                None => c.sort = Some(out),
            }
        }
        match self.place(&base) {
            Place::RootDse => match req.scope {
                Scope::Base => {
                    let attrs = self.root_dse();
                    let tgt_dn = Dn::root();
                    let all = |_: &AttributeType| true;
                    let t = Target {
                        dn: &tgt_dn,
                        attrs: &attrs,
                        can_search: &all,
                    };
                    if filter::eval(&self.schema, &req.filter, &t) == Some(true) {
                        let sel = self.select(attrs, &c.want, c.types_only, &all, true);
                        c.fixed.push_back(Reply::new(ProtocolOp::SearchResultEntry {
                            dn: String::new(),
                            attrs: sel,
                        }));
                    }
                }
                Scope::One => {
                    return fail(&OpError::unwilling(
                        "a one-level search of the root DSE is not supported",
                    ));
                }
                Scope::Sub => {
                    let b = &self.backends[0];
                    for s in b.suffixes() {
                        if let Ok(r) = b.resolve(txn, &self.schema, &self.sys, &s)
                            && let Some(id) = r.entry
                        {
                            c.regions.push_back((0, id, Scope::Sub));
                        }
                    }
                }
            },
            Place::Schema => {
                if req.scope == Scope::One {
                    return fail(&OpError::new(
                        ResultCode::ProtocolError,
                        "the subschema entry has no children",
                    ));
                }
                let attrs = crate::backend::schema::entry(&self.schema);
                self.push_special(&mut c, crate::backend::schema::DN, attrs);
            }
            Place::Monitor => {
                if req.scope == Scope::One {
                    return fail(&OpError::new(
                        ResultCode::NoSuchObject,
                        "cn=monitor has no children",
                    ));
                }
                let attrs = self.counters.entry(now, &self.settings.release);
                self.push_special(&mut c, crate::backend::monitor::DN, attrs);
            }
            Place::UnderMonitor => {
                return fail(&OpError::unwilling("cn=monitor has no entries below it"));
            }
            Place::Outside => return fail(&self.outside(&base)),
            Place::Backend(_) => {
                let found = match self.find(txn, &acting, &base, c.manage) {
                    Ok(Some(f)) => f,
                    Ok(None) => {
                        return Started::Done(vec![done(LdapResult::success(), Vec::new())]);
                    }
                    Err(e) => return fail(&e),
                };
                let found = if deref & 1 != 0 && self.has_class(&found.entry, "alias") {
                    match self.deref(txn, &acting, found) {
                        Ok(f) => f,
                        Err(e) => return fail(&e),
                    }
                } else {
                    found
                };
                if let Some(a) = &ctl.assertion {
                    match self.assert(txn, &acting, found.backend, &found.entry, a) {
                        Ok(()) => {}
                        Err(e) => return fail(&e),
                    }
                }
                c.regions.push_back((found.backend, found.entry.id, req.scope));
            }
        }
        if let Some(p) = ctl.psearch {
            if self.watches.len() >= self.settings.psearch_max as usize {
                return fail(&OpError::new(
                    ResultCode::AdminLimitExceeded,
                    "too many persistent searches",
                ));
            }
            if p.changes_only {
                c.regions.clear();
                c.fixed.clear();
            }
            c.psearch = Some((p, base, req.scope));
        }
        cursors.next += 1;
        let id = cursors.next;
        cursors.map.insert(id, c);
        Started::Cursor(id)
    }

    fn push_special(&self, c: &mut Cursor, dn: &str, attrs: Vec<Attribute>) {
        let d = Dn::parse(dn).expect("a well-known DN");
        let all = |_: &AttributeType| true;
        let t = Target {
            dn: &d,
            attrs: &attrs,
            can_search: &all,
        };
        if filter::eval(&self.schema, &c.filter, &t) == Some(true) {
            let sel = self.select(attrs, &c.want, c.types_only, &all, false);
            c.fixed.push_back(Reply::new(ProtocolOp::SearchResultEntry {
                dn: dn.into(),
                attrs: sel,
            }));
        }
    }

    /// Follow an alias to the entry it names.
    fn deref(
        &self,
        txn: &mut Txn<'_>,
        session: &Session,
        mut f: super::Found,
    ) -> OpResult<super::Found> {
        for _ in 0..16 {
            if !self.has_class(&f.entry, "alias") {
                return Ok(f);
            }
            let Some(target) = f
                .entry
                .get("aliasedObjectName")
                .and_then(|a| a.vals.first())
                .and_then(|v| Dn::parse(&String::from_utf8_lossy(v)).ok())
            else {
                return Err(OpError::new(ResultCode::AliasProblem, "the alias names nothing"));
            };
            f = match self.find(txn, session, &target, false) {
                Ok(Some(x)) => x,
                _ => {
                    return Err(OpError::new(
                        ResultCode::AliasProblem,
                        "the alias names no entry",
                    ));
                }
            };
        }
        Err(OpError::new(
            ResultCode::AliasDereferencingProblem,
            "aliases loop",
        ))
    }

    /// The assertion control (RFC 4528) against an entry.
    pub(crate) fn assert(
        &self,
        txn: &mut Txn<'_>,
        session: &Session,
        b: usize,
        e: &DirEntry,
        f: &Filter,
    ) -> OpResult<()> {
        let mut cache = AclCache::default();
        if self.matches(txn, &mut cache, session, b, e, f)? {
            Ok(())
        } else {
            Err(OpError::new(
                super::error::ASSERTION_FAILED,
                "the assertion is false for the entry",
            ))
        }
    }

    /// Whether `f` is true of entry `e` for `session`.
    pub(crate) fn matches(
        &self,
        txn: &mut Txn<'_>,
        cache: &mut AclCache,
        session: &Session,
        b: usize,
        e: &DirEntry,
        f: &Filter,
    ) -> OpResult<bool> {
        let eff = self.effective_of(txn, cache, b, e)?;
        let key = self.entry_key(&e.dn);
        let mut names = Vec::new();
        filter::names(f, &mut names);
        let need: Vec<&str> = COMPUTED
            .iter()
            .copied()
            .filter(|c| names.iter().any(|n| split_description(n).0.eq_ignore_ascii_case(c)))
            .collect();
        let mut attrs = e.attrs.clone();
        if !need.is_empty() {
            attrs.extend(self.computed(txn, b, e, &eff, &|n| need.iter().any(|x| x.eq_ignore_ascii_case(n)))?);
        }
        let dn = Dn::parse(&e.dn).unwrap_or_else(|_| Dn::root());
        let can = |at: &AttributeType| self.attr_rights(&eff, session, &key, at) & acl::S != 0;
        let t = Target {
            dn: &dn,
            attrs: &attrs,
            can_search: &can,
        };
        Ok(filter::eval(&self.schema, f, &t) == Some(true))
    }

    /// The next match of a cursor, `None` when there are no more.
    fn produce(&self, txn: &mut Txn<'_>, c: &mut Cursor) -> OpResult<Option<(Reply, Vec<Vec<u8>>)>> {
        loop {
            if let Some(r) = c.fixed.pop_front() {
                return Ok(Some((r, Vec::new())));
            }
            if let Some((b, base, scope)) = c.region {
                if let Some(id) = c.pending.pop_front() {
                    if let Some(r) = self.consider(txn, c, b, base, scope, id)? {
                        return Ok(Some(r));
                    }
                    continue;
                }
                if !matches!(c.scan, Scan::Done) {
                    if let Some(d) = c.deadline
                        && self.now() > d
                    {
                        return Err(OpError::new(
                            ResultCode::TimeLimitExceeded,
                            "the search's time limit was reached",
                        ));
                    }
                    let ids = self.backends[b].scan(
                        txn,
                        &self.schema,
                        &c.session,
                        base,
                        scope,
                        &c.scan_filter,
                        &mut c.scan,
                        128,
                    )?;
                    c.pending.extend(ids);
                    continue;
                }
                c.region = None;
            }
            let Some(next) = c.regions.pop_front() else {
                return Ok(None);
            };
            c.region = Some(next);
            c.scan = Scan::Start;
        }
    }

    /// One candidate: a reference, an entry, or nothing.
    #[allow(clippy::too_many_arguments)]
    fn consider(
        &self,
        txn: &mut Txn<'_>,
        c: &mut Cursor,
        b: usize,
        base: u64,
        scope: Scope,
        id: u64,
    ) -> OpResult<Option<(Reply, Vec<Vec<u8>>)>> {
        if !c.seen.insert((b, id)) {
            return Ok(None);
        }
        let Some(mut e) = self.backends[b].entry(txn, &self.schema, &c.session, id)? else {
            return Ok(None);
        };
        if id != base && !c.manage && self.is_referral(&e) {
            let dn = Dn::parse(&e.dn).unwrap_or_else(|_| Dn::root());
            let urls = self.referral_urls(&e, &dn);
            return Ok(Some((Reply::new(ProtocolOp::SearchResultReference(urls)), Vec::new())));
        }
        if id != base && c.deref_search && self.has_class(&e, "alias") {
            let f = super::Found { backend: b, entry: e };
            let Ok(t) = self.deref(txn, &c.session, f) else {
                return Ok(None);
            };
            if t.backend != b || !c.seen.insert((b, t.entry.id)) {
                return Ok(None);
            }
            if scope == Scope::Sub {
                c.regions.push_back((b, t.entry.id, Scope::Sub));
                c.seen.remove(&(b, t.entry.id));
                return Ok(None);
            }
            e = t.entry;
        }
        if !self.matches(txn, &mut c.cache, &c.session, b, &e, &c.filter)? {
            return Ok(None);
        }
        let keys = match &c.sort {
            Some(k) => self.sort_keys(k, &e),
            None => Vec::new(),
        };
        let attrs = self.view(txn, &mut c.cache, b, &e, &c.session, &c.want, c.types_only)?;
        Ok(Some((
            Reply::new(ProtocolOp::SearchResultEntry { dn: e.dn, attrs }),
            keys,
        )))
    }

    fn sort_keys(&self, keys: &[SortKey], e: &DirEntry) -> Vec<Vec<u8>> {
        keys.iter()
            .map(|k| {
                let mut vals: Vec<Vec<u8>> = e
                    .attrs
                    .iter()
                    .filter(|a| {
                        self.schema
                            .attribute(split_description(&a.ty).0)
                            .is_some_and(|t| t.oid() == k.oid)
                    })
                    .flat_map(|a| a.vals.iter())
                    .map(|v| {
                        k.rule
                            .or(k.eq)
                            .and_then(|r| r.normalize(&self.schema, v).ok())
                            .unwrap_or_else(|| v.clone())
                    })
                    .collect();
                vals.sort();
                // The least value ascending, the greatest descending;
                // none sorts after every value ([0xFF] marks it).
                let pick = if k.reverse { vals.pop() } else { vals.into_iter().next() };
                pick.map_or_else(|| vec![0xFF, 0xFF], |mut v| {
                    v.insert(0, 0);
                    v
                })
            })
            .collect()
    }

    /// `dir_next`: the next entries of cursor `id`.
    pub fn next(&mut self, txn: &mut Txn<'_>, cursors: &mut Cursors, id: u64, budget: Budget) -> Page {
        let Some(mut c) = cursors.map.remove(&id) else {
            return Page {
                replies: Vec::new(),
                state: PageState::Done,
            };
        };
        let mut replies = Vec::new();
        let mut bytes = 0;
        let end = loop {
            if replies.len() >= budget.entries.max(1) || (bytes >= budget.bytes && !replies.is_empty()) {
                cursors.map.insert(id, c);
                return Page {
                    replies,
                    state: PageState::More,
                };
            }
            if let Some(p) = c.page_size
                && c.page_sent >= p
            {
                let ctl = PagedResults {
                    size: 0,
                    cookie: id.to_be_bytes().to_vec(),
                }
                .to_control(false);
                replies.push(done(LdapResult::success(), vec![ctl]));
                c.paused = true;
                cursors.map.insert(id, c);
                return Page {
                    replies,
                    state: PageState::Paused,
                };
            }
            if let Some(d) = c.deadline
                && self.now() > d
            {
                break Err(OpError::new(
                    ResultCode::TimeLimitExceeded,
                    "the search's time limit was reached",
                ));
            }
            let next = if c.sort.is_some() {
                if c.sorted.is_none() {
                    match self.collect_sorted(txn, &mut c) {
                        Ok(v) => c.sorted = Some(v),
                        Err(e) => break Err(e),
                    }
                }
                Ok(c.sorted.as_mut().and_then(VecDeque::pop_front).map(|r| (r, Vec::new())))
            } else {
                self.produce(txn, &mut c)
            };
            match next {
                Err(e) => break Err(e),
                Ok(None) => break Ok(()),
                Ok(Some((r, _))) => {
                    let is_entry = matches!(r.op, ProtocolOp::SearchResultEntry { .. });
                    if is_entry && c.size_limit > 0 && c.sent >= c.size_limit {
                        break Err(OpError::new(
                            ResultCode::SizeLimitExceeded,
                            "more entries match than the size limit allows",
                        ));
                    }
                    if let ProtocolOp::SearchResultEntry { dn, attrs } = &r.op {
                        bytes += size_of(dn, attrs);
                        c.sent += 1;
                        c.page_sent += 1;
                        self.counters.entries_sent += 1;
                    }
                    replies.push(r);
                }
            }
        };
        if end.is_ok()
            && let Some((p, base, scope)) = c.psearch.take()
        {
            let w = self.watches.add(super::watch::Watch::new(
                c.session.clone(),
                base,
                scope,
                c.filter.clone(),
                c.want.clone(),
                c.types_only,
                p,
            ));
            self.counters.current_psearches = self.watches.len() as u64;
            return Page {
                replies,
                state: PageState::Watching(w),
            };
        }
        let mut ctls = Vec::new();
        if let Some(s) = c.sort_ctl.take() {
            ctls.push(s);
        } else if c.sort.is_some() {
            ctls.push(
                SortResponse {
                    result: ResultCode::Success,
                    attr: None,
                }
                .to_control(false),
            );
        }
        if c.page_size.is_some() {
            ctls.push(PagedResults::default().to_control(false));
        }
        let result = match end {
            Ok(()) => LdapResult::success(),
            Err(e) => e.result(),
        };
        replies.push(done(result, ctls));
        self.counters.searches_completed += 1;
        self.counters.ops_completed += 1;
        Page {
            replies,
            state: PageState::Done,
        }
    }

    fn collect_sorted(&self, txn: &mut Txn<'_>, c: &mut Cursor) -> OpResult<VecDeque<Reply>> {
        let mut all = Vec::new();
        while let Some((r, k)) = self.produce(txn, c)? {
            all.push((k, r));
        }
        let keys = c.sort.as_ref().map(|v| v.iter().map(|k| k.reverse).collect::<Vec<_>>());
        let rev = keys.unwrap_or_default();
        all.sort_by(|(a, _), (b, _)| {
            for (i, r) in rev.iter().enumerate() {
                let (x, y) = (a.get(i), b.get(i));
                let o = x.cmp(&y);
                let o = if *r {
                    // Absent values stay last when reversed too.
                    match (x.is_some_and(|v| v[0] == 0xFF), y.is_some_and(|v| v[0] == 0xFF)) {
                        (true, false) => Ordering::Greater,
                        (false, true) => Ordering::Less,
                        _ => o.reverse(),
                    }
                } else {
                    o
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        Ok(all.into_iter().map(|(_, r)| r).collect())
    }
}
