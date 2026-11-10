//! QS/400: leases shared with what is outside the server. On QS/400 each
//! client connection is served by a server in a process of its own, and
//! other processes (other servers, FTP, programs) change the same files.
//! A [`LeaseObserver`] hears of every change to the leases on a file, a
//! backend's [`crate::Handle::lease_limit`] caps a lease by what the
//! others hold before it is granted, and [`crate::SmbServer::break_leases`]
//! breaks a file's leases for a change made outside, as a change through
//! another open would.

use std::sync::Arc;

use crate::files::LeaseId;

/// A lease's identity: the client's GUID and the lease key it chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseKey {
    /// ClientGuid of the client that holds it.
    pub client: [u8; 16],
    /// LeaseKey of its CREATE.
    pub key: [u8; 16],
}

impl LeaseKey {
    /// The 32 bytes: the client GUID, then the lease key.
    pub fn to_bytes(self) -> [u8; 32] {
        let mut b = [0; 32];
        b[..16].copy_from_slice(&self.client);
        b[16..].copy_from_slice(&self.key);
        b
    }
}

impl From<LeaseId> for LeaseKey {
    fn from(id: LeaseId) -> Self {
        Self {
            client: *id.client.as_bytes(),
            key: id.key,
        }
    }
}

/// Hears of every change to the leases on a file of a share.
pub trait LeaseObserver: Send + Sync + 'static {
    /// The file `file_index` on the share `share` now has `leases`, each
    /// with its caching bits (`SMB2_LEASE_*_CACHING`), and no others. A
    /// lease breaking keeps its old state until the client acknowledges.
    /// Called with the file's state locked: it must not block or call
    /// back into the server.
    fn leases(&self, share: &str, file_index: u64, leases: &[(LeaseKey, u32)]);
}

/// The observer the server reports to.
#[derive(Clone)]
pub(crate) struct ObserverRef(pub(crate) Arc<dyn LeaseObserver>);

impl std::fmt::Debug for ObserverRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LeaseObserver")
    }
}
