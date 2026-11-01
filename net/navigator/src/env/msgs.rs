// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message queues (`QSYSOPR`, the user's own) and the history log `QHST`.

/// What the message pages need.
pub trait Msgs: Send + Sync {}
