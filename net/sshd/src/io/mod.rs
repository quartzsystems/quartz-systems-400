// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server's access to the IFS, whole-file helpers, and in-memory
//! pipes.

pub mod files;
pub mod pipe;

use ifs::client::IfsClient;

use crate::auth::Account;

/// The IFS as the server reaches it. An [`IfsClient`] is not `Send`
/// (the service's link belongs to one thread), so each is made on the
/// thread that uses it.
pub trait FileSystem: Send + Sync {
    /// The IFS as the server itself: configuration, host keys, banners
    /// and every user's `authorized_keys`.
    fn system(&self) -> Box<dyn IfsClient>;
    /// The IFS as `account`'s job sees it (SFTP and scp run with the
    /// user's authority); `None` when no job can be had for it.
    fn user(&self, account: &Account) -> Option<Box<dyn IfsClient>>;
}
