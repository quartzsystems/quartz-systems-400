// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The uninterruptible power supply as the hypervisor reports it: the
//! host's UPS daemon (or its operator) sends `qs400-set-power-source`
//! when utility power fails or returns, the agent keeps the state in
//! [`STATE_FILE`], and the storage service acts on it (`QUPSDLYTIM`,
//! `QUPSMSGQ`, `CPF1816`, `CPF1817`). A line per fact: `battery` or
//! `utility`, then `low` when the battery is about to run out.

/// Where the agent keeps the state.
pub const STATE_FILE: &str = "/run/qs400/ups";

/// What the power supply says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Power {
    /// Utility power failed: the system runs on the UPS.
    pub on_battery: bool,
    /// The UPS battery is about to run out.
    pub battery_low: bool,
}

impl Power {
    /// The state file's text.
    #[must_use]
    pub fn encode(self) -> String {
        let mut s = String::from(if self.on_battery { "battery\n" } else { "utility\n" });
        if self.battery_low {
            s.push_str("low\n");
        }
        s
    }

    /// From the state file's text; anything else is utility power.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let words: Vec<&str> = text.split_whitespace().collect();
        Self {
            on_battery: words.contains(&"battery"),
            battery_low: words.contains(&"low"),
        }
    }

    /// The state file, utility power when there is none.
    #[must_use]
    pub fn read() -> Self {
        std::fs::read_to_string(STATE_FILE).map_or_else(|_| Self::default(), |t| Self::decode(&t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for p in [
            Power::default(),
            Power { on_battery: true, battery_low: false },
            Power { on_battery: true, battery_low: true },
        ] {
            assert_eq!(Power::decode(&p.encode()), p);
        }
        assert_eq!(Power::decode(""), Power::default());
    }
}
