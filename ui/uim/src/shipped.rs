// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped system panel groups under `pnlgrp/`: the panels behind
//! the system's displays (`WRKACTJOB`, `DSPJOB`, ...), each a `*PNLGRP`
//! in `QSYS` installed when missing. The list is written by `build.rs`.
//! Every word in them is Quartz's; the geometry follows the goldens
//! (`docs/UIM.md` §7).

include!(concat!(env!("OUT_DIR"), "/shipped_list.rs"));

/// The source of one shipped panel group.
#[must_use]
pub fn source(name: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shipped_group_compiles() {
        for (name, text) in FILES {
            let c = crate::compile(
                text,
                &crate::NoImbeds,
                &crate::NoImports,
                &crate::CompileOpts::default(),
            );
            assert!(c.ok(), "{name}:\n{}", c.listing);
        }
    }
}
