// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The names of what a line gets at vary on. IBM names the network
//! controller and the TCP/IP device after the line: the first five
//! characters of the line name and `NET` or `TCP` (`LINETH` gives
//! `LINETNET` and `LINETTCP`), with a two-digit suffix from `00` when
//! the name is taken (`LINETNET00`, `LINETNET01`, ...), as the pub400
//! capture shows (`tools/conformance/golden/phase10/cmn`).

/// The object type code of a `*LIND` (the store's registry).
pub const LIND_TYPE: u16 = 0x001D;

/// The object type code of a `*CTLD`.
pub const CTLD_TYPE: u16 = 0x001C;

/// The controller type of a network controller.
pub const NET_CTL: &str = "*NET";

/// The device class of a network device.
pub const NET_DEV_CLASS: &str = "*NET";

/// The device type of the TCP/IP device under a network controller.
pub const TCPIP_DEV: &str = "*TCPIP";

/// The stem of a line's controller and device names: its first five
/// characters.
fn stem(line: &str) -> &str {
    let end = line.char_indices().nth(5).map_or(line.len(), |(i, _)| i);
    &line[..end]
}

/// The name of `line`'s controller or device with `suffix` (`NET`,
/// `TCP`), the first one `taken` does not refuse: `LINETNET`, then
/// `LINETNET00` to `LINETNET99`. `None` when every name is taken.
pub fn derived(line: &str, suffix: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    let base = format!("{}{suffix}", stem(line));
    if !taken(&base) {
        return Some(base);
    }
    (0..100)
        .map(|n| format!("{base}{n:02}"))
        .find(|name| !taken(name))
}

/// The controller name for `line`.
pub fn controller(line: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    derived(line, "NET", taken)
}

/// The TCP/IP device name for `line`.
pub fn device(line: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    derived(line, "TCP", taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_capture() {
        assert_eq!(controller("LINETH", |_| false).as_deref(), Some("LINETNET"));
        assert_eq!(device("LINETH", |_| false).as_deref(), Some("LINETTCP"));
        assert_eq!(
            controller("ETHLINE", |_| false).as_deref(),
            Some("ETHLINET")
        );
        assert_eq!(controller("ETH", |_| false).as_deref(), Some("ETHNET"));
    }

    #[test]
    fn suffixes_when_taken() {
        let taken = |n: &str| matches!(n, "LINETNET" | "LINETNET00" | "LINETNET01");
        assert_eq!(controller("LINETH", taken).as_deref(), Some("LINETNET02"));
        assert_eq!(
            device("LINETH", |n| n == "LINETTCP").as_deref(),
            Some("LINETTCP00")
        );
        assert_eq!(controller("LINETH", |_| true), None);
    }
}
