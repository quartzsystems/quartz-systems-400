// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel side: [`Pinger`] (an [`crate::ping::Echoer`]),
//! [`RouteProber`] (a [`crate::route::Prober`]) and [`Capture`]. On
//! Linux they are sockets; elsewhere opening one fails, so the logic
//! stays testable on any host.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod raw;

#[cfg(target_os = "linux")]
pub use linux::{Capture, Pinger, ProbeKind, RouteProber, ifindex};

/// vsock, for the guest agent (`lic/guestagent`).
#[cfg(target_os = "linux")]
pub mod vsock {
    pub use super::raw::{accept, vsock_listen as listen};
}

#[cfg(not(target_os = "linux"))]
mod host;

#[cfg(not(target_os = "linux"))]
pub use host::{Capture, Pinger, ProbeKind, RouteProber, ifindex};
