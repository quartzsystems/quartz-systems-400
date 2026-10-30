// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message identifiers of lines, controllers and devices, as the
//! pub400 `QCPFMSG` lists them (`tools/conformance/oracle/net/pub400/
//! messages-cpf.txt`), and the data each takes in its layout there.
//! The texts are Quartz's (`runtime/msgf/msgs/qcpfmsg.toml`). Which of
//! them IBM sends on a lost carrier is an open question
//! (`docs/NET_OPEN_QUESTIONS.md`, "Communications objects").

/// Line description created (`&1` line).
pub const LINE_CREATED: &str = "CPC2601";
/// Line description changed.
pub const LINE_CHANGED: &str = "CPC2615";
/// Line description deleted.
pub const LINE_DELETED: &str = "CPC2604";
/// Device description deleted.
pub const DEV_DELETED: &str = "CPC2602";
/// Controller description deleted.
pub const CTL_DELETED: &str = "CPC2603";
/// Vary on completed for a device.
pub const DEV_VARIED_ON: &str = "CPC2605";
/// Vary off completed for a device.
pub const DEV_VARIED_OFF: &str = "CPC2606";
/// Vary on completed for a line.
pub const LINE_VARIED_ON: &str = "CPC2607";
/// Vary off completed for a line.
pub const LINE_VARIED_OFF: &str = "CPC2608";
/// Vary on completed for a controller.
pub const CTL_VARIED_ON: &str = "CPC2609";
/// Vary off completed for a controller.
pub const CTL_VARIED_OFF: &str = "CPC2610";
/// Automatic configuration created a controller description.
pub const AUTO_CTL: &str = "CPC2629";
/// Automatic configuration created a device description.
pub const AUTO_DEV: &str = "CPC2630";
/// Automatic configuration deleted a controller description.
pub const AUTO_CTL_DELETED: &str = "CPC2631";
/// Automatic configuration deleted a device description.
pub const AUTO_DEV_DELETED: &str = "CPC2632";

/// Automatic configuration could not make a controller.
pub const AUTO_CTL_FAILED: &str = "CPF2609";
/// Automatic configuration could not make a device.
pub const AUTO_DEV_FAILED: &str = "CPF2611";

/// Line description not found.
pub const LINE_NOT_FOUND: &str = "CPF2601";
/// Controller not found.
pub const CTL_NOT_FOUND: &str = "CPF2602";
/// Device description not found.
pub const DEV_NOT_FOUND: &str = "CPF2603";
/// Line description already exists.
pub const LINE_EXISTS: &str = "CPF2606";
/// Line description not changed.
pub const LINE_NOT_CHANGED: &str = "CPF261E";
/// Line description must be varied off.
pub const MUST_VARY_OFF: &str = "CPF2633";
/// A line cannot be deleted while varied on.
pub const DELETE_VARIED_ON: &str = "CPF26AB";
/// The vary command was not processed (the escape after a diagnostic).
pub const VARY_NOT_PROCESSED: &str = "CPF2640";

/// Line already varied on.
pub const LINE_ALREADY_ON: &str = "CPD2645";
/// Line already varied off.
pub const LINE_ALREADY_OFF: &str = "CPD2646";
/// Controller already varied off.
pub const CTL_ALREADY_OFF: &str = "CPD2650";
/// Device already varied off.
pub const DEV_ALREADY_OFF: &str = "CPD2640";
/// Line resource name not found.
pub const RSRC_NOT_FOUND: &str = "CPD26D4";
/// A line could not be varied on (the kernel refused).
pub const LINE_NOT_VARIED_ON: &str = "CPD2659";
/// Controller not varied on (its line is off).
pub const CTL_NOT_ON: &str = "CPF5928";
/// Line not varied on.
pub const LINE_NOT_ON: &str = "CPF5933";

/// Controller contacted on the line: carrier present (`&1` controller,
/// `&2` line).
pub const CONTACTED: &str = "CPF5908";
/// Automatic error recovery has started (`&1` controller): carrier
/// lost within the recovery limit (`RCYPND`).
pub const RECOVERY_STARTED: &str = "CPI5917";
/// Line failed, probable cabling or hardware problem; an inquiry
/// (`C G R`) in the communications layout (`&23` line, `&24`
/// controller): carrier lost past the limit (`RCYCNL`) or with no
/// recovery (`FAILED`).
pub const LINE_FAILED: &str = "CPA58EE";

/// `text` in `n` CP037 bytes, blank padded.
fn field(text: &str, n: usize) -> Vec<u8> {
    wm::rec::text_bytes(text, n)
}

/// The 40-variable layout the `CPA58xx` and many `CPI59xx` messages
/// share: `&1` a byte, `&22` four hex bytes, `&23` to `&30` ten
/// characters (line, controller, device, mode, ..., network
/// interface), `&31` two, `&32` eight, `&40` four hex bytes.
#[must_use]
pub fn comm_data(line: &str, ctl: &str, dev: &str) -> Vec<u8> {
    let mut out = field("", 1);
    out.extend_from_slice(&[0; 4]);
    out.extend(field(line, 10));
    out.extend(field(ctl, 10));
    out.extend(field(dev, 10));
    for _ in 26..=30 {
        out.extend(field("", 10));
    }
    out.extend(field("", 2));
    out.extend(field("", 8));
    out.extend_from_slice(&[0; 4]);
    out
}

/// The data of `msgid` about `line` and its controller `ctl`.
#[must_use]
pub fn data(msgid: &str, line: &str, ctl: &str) -> Vec<u8> {
    match msgid {
        CONTACTED => [field(ctl, 10), field(line, 10), field("", 5)].concat(),
        RECOVERY_STARTED => field(if ctl.is_empty() { line } else { ctl }, 10),
        LINE_FAILED => comm_data(line, ctl, ""),
        _ => field(line, 10),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_match_the_capture() {
        // &1 1, &22 4, &23-&30 8 x 10, &31 2, &32 8, &40 4.
        assert_eq!(comm_data("L", "C", "D").len(), 1 + 4 + 80 + 2 + 8 + 4);
        let d = data(LINE_FAILED, "ETHLINE", "ETHLINET");
        assert_eq!(wm::rec::text_of(&d[5..15]), "ETHLINE");
        assert_eq!(wm::rec::text_of(&d[15..25]), "ETHLINET");
        let c = data(CONTACTED, "ETHLINE", "ETHLINET");
        assert_eq!(c.len(), 25);
        assert_eq!(wm::rec::text_of(&c[..10]), "ETHLINET");
        assert_eq!(wm::rec::text_of(&c[10..20]), "ETHLINE");
        assert_eq!(
            wm::rec::text_of(&data(RECOVERY_STARTED, "ETHLINE", "")),
            "ETHLINE"
        );
    }
}
