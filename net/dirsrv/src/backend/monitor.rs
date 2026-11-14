// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `monitor` backend (docs/DIRSRV.md §8): `cn=monitor`, one
//! read-only entry with the counters IBM's server shows that QS/400
//! keeps; those it does not keep (caches, work queues, transactions,
//! event notification) are absent, not zero. The operations count in the
//! service; the job reports its connection counts with
//! [`Counters::connections`].

use ldap::msg::Attribute;

use crate::ops::clock::gentime;

/// The suffix.
pub const DN: &str = "cn=monitor";

/// The counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Counters {
    pub ops_initiated: u64,
    pub ops_completed: u64,
    pub binds_requested: u64,
    pub binds_completed: u64,
    pub unbinds_requested: u64,
    pub unbinds_completed: u64,
    pub adds_requested: u64,
    pub adds_completed: u64,
    pub deletes_requested: u64,
    pub deletes_completed: u64,
    pub modifies_requested: u64,
    pub modifies_completed: u64,
    pub modrdns_requested: u64,
    pub modrdns_completed: u64,
    pub compares_requested: u64,
    pub compares_completed: u64,
    pub searches_requested: u64,
    pub searches_completed: u64,
    pub extops_requested: u64,
    pub extops_completed: u64,
    pub abandons_requested: u64,
    pub abandons_completed: u64,
    pub unknown_requested: u64,
    pub unknown_completed: u64,
    pub entries_sent: u64,
    pub current_psearches: u64,
    pub psearch_processed: u64,
    pub current_connections: u64,
    pub total_connections: u64,
    pub total_tls_connections: u64,
    pub total_ssl_connections: u64,
    pub max_connections: u64,
    /// When the instance started, Unix microseconds.
    pub start_time: u64,
}

impl Counters {
    /// The job's connection counts.
    pub fn connections(&mut self, current: u64, total: u64, tls: u64, ssl: u64) {
        self.current_connections = current;
        self.total_connections = total;
        self.total_tls_connections = tls;
        self.total_ssl_connections = ssl;
    }

    /// The entry's attributes at `now` (Unix microseconds); `version`
    /// names QS/400's release.
    #[must_use]
    pub fn entry(&self, now: u64, version: &str) -> Vec<Attribute> {
        let n = |ty: &str, v: u64| Attribute::new(ty, &[&v.to_string()]);
        vec![
            Attribute::new("objectClass", &["top", "extensibleObject"]),
            Attribute::new("cn", &["monitor"]),
            Attribute::new("version", &[&format!("QS/400 Directory Server, {version}")]),
            n("opsinitiated", self.ops_initiated),
            n("opscompleted", self.ops_completed),
            n("bindsrequested", self.binds_requested),
            n("bindscompleted", self.binds_completed),
            n("unbindsrequested", self.unbinds_requested),
            n("unbindscompleted", self.unbinds_completed),
            n("addsrequested", self.adds_requested),
            n("addscompleted", self.adds_completed),
            n("deletesrequested", self.deletes_requested),
            n("deletescompleted", self.deletes_completed),
            n("modifiesrequested", self.modifies_requested),
            n("modifiescompleted", self.modifies_completed),
            n("modrdnsrequested", self.modrdns_requested),
            n("modrdnscompleted", self.modrdns_completed),
            n("comparesrequested", self.compares_requested),
            n("comparescompleted", self.compares_completed),
            n("searchesrequested", self.searches_requested),
            n("searchescompleted", self.searches_completed),
            n("extopsrequested", self.extops_requested),
            n("extopscompleted", self.extops_completed),
            n("abandonsrequested", self.abandons_requested),
            n("abandonscompleted", self.abandons_completed),
            n("unknownopsrequested", self.unknown_requested),
            n("unknownopscompleted", self.unknown_completed),
            n("entriessent", self.entries_sent),
            n("currentpersistentsearches", self.current_psearches),
            n("persistentsearchprocessedchanges", self.psearch_processed),
            n("currentconnections", self.current_connections),
            n("totalconnections", self.total_connections),
            n("total_tls_connections", self.total_tls_connections),
            n("total_ssl_connections", self.total_ssl_connections),
            n("maxconnections", self.max_connections),
            Attribute::new("starttime", &[&gentime(self.start_time)]),
            Attribute::new("currenttime", &[&gentime(now)]),
        ]
    }
}
