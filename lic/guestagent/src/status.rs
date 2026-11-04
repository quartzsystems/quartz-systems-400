// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The state file `slic` rewrites at each SRC it shows
//! ([`PATH`]): one `key=value` line each. The agent reads it; nothing
//! else keeps the IPL's state outside PID 1.

use std::fmt::Write as _;

/// Where `slic` keeps it.
pub const PATH: &str = "/run/qs400/status";

/// The system's state as `slic` last wrote it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The system name (`QSHQ400`).
    pub system: String,
    /// The release (`V1R1M0`).
    pub release: String,
    /// `ipl`, `complete`, `ending` or `down`.
    pub state: String,
    /// `normal` or `manual`.
    pub mode: String,
    /// The IPL type (`A`, `B`, `D`).
    pub ipl_type: String,
    /// The LIC slot booted.
    pub slot: String,
    /// The last SRC, eight characters.
    pub src: String,
    /// Its text.
    pub src_text: String,
    /// When the IPL started, Unix seconds.
    pub ipl_time: u64,
}

impl Status {
    /// As the file holds it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        let mut line = |k: &str, v: &str| {
            let v = v.replace(['\n', '\r'], " ");
            let _ = writeln!(s, "{k}={v}");
        };
        line("system", &self.system);
        line("release", &self.release);
        line("state", &self.state);
        line("mode", &self.mode);
        line("ipl_type", &self.ipl_type);
        line("slot", &self.slot);
        line("src", &self.src);
        line("src_text", &self.src_text);
        line("ipl_time", &self.ipl_time.to_string());
        s
    }

    /// From the file; unknown keys are passed over.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for l in text.lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            let v = v.to_string();
            match k {
                "system" => s.system = v,
                "release" => s.release = v,
                "state" => s.state = v,
                "mode" => s.mode = v,
                "ipl_type" => s.ipl_type = v,
                "slot" => s.slot = v,
                "src" => s.src = v,
                "src_text" => s.src_text = v,
                "ipl_time" => s.ipl_time = v.parse().unwrap_or(0),
                _ => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Status {
            system: "QSHQ400".into(),
            release: "V1R1M0".into(),
            state: "complete".into(),
            mode: "normal".into(),
            ipl_type: "B".into(),
            slot: "A".into(),
            src: "C9002A00".into(),
            src_text: "IPL complete\nnext".into(),
            ipl_time: 1_793_000_000,
        };
        let back = Status::parse(&s.render());
        assert_eq!(back.src_text, "IPL complete next");
        assert_eq!(Status { src_text: "IPL complete next".into(), ..s }, back);
        assert_eq!(Status::parse("x\nsrc=A1\nzz=1"), Status { src: "A1".into(), ..Status::default() });
    }
}
