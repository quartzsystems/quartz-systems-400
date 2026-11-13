// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the file system tells a server job between requests: queued
//! byte-range locks now granted (NLM's `NLM_GRANTED`) and lease break
//! orders (`NFSv4` `CB_RECALL`). The job's poll is
//! `IfsClient::lease_orders`; [`ClientNews`] is it over any client.

use ifs::client::IfsClient;
use ifs::lease::News;

use super::error::{VResult, VfsError};

/// A poll for [`News`].
pub trait NewsSource {
    /// What happened, waiting up to `wait_ms` for something.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn poll(&mut self, wait_ms: u32) -> VResult<News>;
}

/// [`NewsSource`] over an IFS client of the server's job.
#[derive(Debug)]
pub struct ClientNews<C: IfsClient>(pub C);

impl<C: IfsClient> NewsSource for ClientNews<C> {
    fn poll(&mut self, wait_ms: u32) -> VResult<News> {
        match self.0.lease_orders(wait_ms) {
            Ok(n) => Ok(n),
            Err(ifs::Errno::EAGAIN | ifs::Errno::ETIME) => Ok(News::default()),
            Err(e) => Err(VfsError::from(e)),
        }
    }
}
