// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display-file DDS for language servers: the `dds` front end's
//! declarations and references, with `CRTDSPF`'s own diagnostics.

use dds::analysis::{Analysis, Source};

use crate::{CompileOpts, NoRefs, RefResolver, compile_dspf};

/// Analyze display-file DDS; `refs` answers `REF` and `REFFLD`.
#[must_use]
pub fn analyze(source: &Source, refs: Option<&dyn RefResolver>) -> Analysis {
    let resolver: &dyn RefResolver = refs.unwrap_or(&NoRefs);
    dds::analysis::analyze_with(source, |src| {
        let c = compile_dspf(src, resolver, &CompileOpts::default());
        (None, c.diagnostics)
    })
}
