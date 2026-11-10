//! QS/400 addition: the application admits each tree connect and reads
//! the sessions the server holds (see `QS400-PATCHES.md`). NetServer
//! counts a share's users across the jobs that serve its clients, so a
//! share's maximum users is checked outside the server.

use std::net::SocketAddr;

/// A tree connect to admit.
#[derive(Debug, Clone)]
pub struct TreeRequest<'a> {
    /// The connection's client.
    pub peer: Option<SocketAddr>,
    /// The session's SessionId.
    pub session_id: u64,
    /// The session's user name; empty for an anonymous session.
    pub user: &'a str,
    /// The session is a guest's (no key).
    pub guest: bool,
    /// The share as the server names it (`IPC$` for the pipe share).
    pub share: &'a str,
    /// The share type the response gives: 1 disk, 2 pipe, 3 print.
    pub kind: u8,
    /// The TreeId the tree gets when admitted.
    pub tree_id: u32,
}

/// Admits tree connects. Called during TREE_CONNECT after the share's own
/// checks pass; `false` fails it with STATUS_REQUEST_NOT_ACCEPTED. The
/// call blocks the connection's task; keep it short.
pub trait TreeAdmission: Send + Sync + 'static {
    /// Whether `req` may connect.
    fn admit(&self, req: &TreeRequest<'_>) -> bool;
}

/// A shared admission, as the server's configuration holds it.
#[derive(Clone)]
pub(crate) struct AdmissionRef(pub(crate) std::sync::Arc<dyn TreeAdmission>);

impl std::fmt::Debug for AdmissionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TreeAdmission")
    }
}

/// A tree connect as [`crate::SmbServer::sessions`] shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeView {
    /// Its TreeId.
    pub tree_id: u32,
    /// The share.
    pub share: String,
    /// 1 disk, 2 pipe, 3 print.
    pub kind: u8,
    /// Files and pipes open through it.
    pub opens: u32,
}

/// A session as [`crate::SmbServer::sessions`] shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    /// The connection's client.
    pub peer: Option<SocketAddr>,
    /// Its SessionId.
    pub session_id: u64,
    /// Its user name; empty for an anonymous session.
    pub user: String,
    /// A guest's session (no key).
    pub guest: bool,
    /// Its tree connects.
    pub trees: Vec<TreeView>,
}
