// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSDEMO's release (its own `VERSION`) and build ID, under `QSDEMO_`.

use std::path::Path;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the fixture's root");
    vrm::build::capture(repo, "QSDEMO_");
}
