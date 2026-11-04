// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The agent's channel in sysfs. QS/400 runs no udev, so there is no
//! `/dev/virtio-ports/<name>` link: the port is found by the `name`
//! attribute of each `class/virtio-ports/vportNpM` and opened as
//! `/dev/vportNpM`. vsock is present when a virtio-vsock device is bound.

use std::fs;
use std::path::{Path, PathBuf};

/// A virtio-serial port by its name: the device node's name
/// (`vport1p1`), `None` when no port has it.
#[must_use]
pub fn find(sys: &Path, name: &str) -> Option<String> {
    let dir = sys.join("class/virtio-ports");
    let mut ports: Vec<String> = fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ports.sort();
    ports
        .into_iter()
        .find(|p| fs::read_to_string(dir.join(p).join("name")).is_ok_and(|n| n.trim() == name))
}

/// The device node of port `node` under `/dev`.
#[must_use]
pub fn node(dev: &Path, node: &str) -> PathBuf {
    dev.join(node)
}

/// Whether the hypervisor offers vsock: a virtio-vsock device is bound to
/// its transport.
#[must_use]
pub fn vsock(sys: &Path) -> bool {
    fs::read_dir(sys.join("bus/virtio/drivers/vmw_vsock_virtio_transport"))
        .is_ok_and(|rd| rd.filter_map(Result::ok).any(|e| e.file_name().to_string_lossy().starts_with("virtio")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_by_name() {
        let root = std::env::temp_dir().join(format!("qsga-port-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for (p, n) in [("vport1p1", "com.redhat.spice.0"), ("vport1p2", "org.qemu.guest_agent.0")] {
            let d = root.join("class/virtio-ports").join(p);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("name"), format!("{n}\n")).unwrap();
        }
        assert_eq!(find(&root, crate::PORT_NAME).as_deref(), Some("vport1p2"));
        assert_eq!(find(&root, "nothing"), None);
        assert_eq!(find(&root.join("none"), crate::PORT_NAME), None);
        assert!(!vsock(&root));
        assert_eq!(node(Path::new("/dev"), "vport1p2"), PathBuf::from("/dev/vport1p2"));
        let _ = fs::remove_dir_all(&root);
    }
}
