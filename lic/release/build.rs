// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Captures (through `vrm::build`) the build ID (`git describe --always --dirty`) into the
//! `QS400_BUILD_ID` environment variable for `env!`. An explicit
//! `QS400_BUILD_ID` in the environment wins, so a release build can pin it.
//! Without git the ID is `unknown`.
//!
//! `QS400_RELEASE` (`VxRyMz`) makes a test build claim another release,
//! for the upgrade path (docs/INSTALL.md); absent, the release follows
//! `VERSION` at the repository root (`1.1.0` is V1R1M0, docs/releasing.md).

use std::path::Path;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root");
    vrm::build::capture(repo, "QS400_");
}
