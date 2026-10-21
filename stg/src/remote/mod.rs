// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime's sources, answered by the service.

mod data;
mod objects;
mod provider;

pub use data::{RemoteData, RemoteIo, status};
pub use objects::{RemoteMsgs, RemoteObjects, RemoteSpaces};
pub use provider::RemoteProvider;

use crate::Shared;

/// Every source over one link, ready to give a job.
pub struct Sources {
    /// Programs and system pointers.
    pub objects: RemoteObjects,
    /// User spaces.
    pub spaces: RemoteSpaces,
    /// Message descriptions.
    pub msgs: RemoteMsgs,
    /// Data areas and queues.
    pub data: RemoteData,
    /// Files.
    pub io: RemoteIo,
    /// The APIs' provider.
    pub provider: RemoteProvider,
}

impl Sources {
    /// All of them over `link`.
    #[must_use]
    pub fn new(link: &Shared) -> Self {
        Self {
            objects: RemoteObjects::new(link.clone()),
            spaces: RemoteSpaces::new(link.clone()),
            msgs: RemoteMsgs::new(link.clone()),
            data: RemoteData::new(link.clone()),
            io: RemoteIo::new(link.clone()),
            provider: RemoteProvider::new(link.clone()),
        }
    }
}
