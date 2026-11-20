// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where in the IPL a code belongs.

/// Where in the IPL a code belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// LIC initialization.
    Lic,
    /// Operating system initialization.
    Os,
    /// Attention, continues.
    Attention,
    /// Fatal, parks.
    Fatal,
    /// Power down.
    PowerDown,
}
