// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server's pictures of the fixture screens against the prototype's
//! PNGs, byte for byte.

use accessweb::fixture::session5250 as fx;

fn golden(name: &str) -> Vec<u8> {
    let p = format!("{}/tests/golden/shared/img/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

#[test]
fn every_golden_png_byte_for_byte() {
    for (name, view, scale) in fx::pictures() {
        let ours = view.png(scale);
        let want = golden(name);
        if ours != want {
            std::fs::write(std::env::temp_dir().join(format!("ours-{name}")), &ours).unwrap();
        }
        assert_eq!(ours.len(), want.len(), "{name}");
        assert!(ours == want, "{name} differs");
    }
}
