// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The download page's files under /QQS/ProdData/Access/Web/download
//! (handoff step 21). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// The download page's files under /QQS/ProdData/Access/Web/download.
pub trait Download: Send + Sync {}
