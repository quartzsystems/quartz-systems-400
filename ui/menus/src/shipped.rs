// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped panel groups: the generated command menus under
//! `pnlgrp/` and the hand-written task menus under `sys/`, each the
//! panel group behind the `*MENU` of the same name in `QSYS`. The list
//! is written by `build.rs`.

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
    fn main_menu_is_shipped_and_compiles() {
        let text = source("MAIN").expect("MAIN");
        let c = uim::compile(text, &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        assert!(c.ok(), "{}", c.listing);
        let g = c.group.unwrap();
        let p = g.panel("MAIN").expect("a MAIN panel");
        assert!(p.cmdline().is_some());
        assert!(FILES.len() > 500);
    }
}
