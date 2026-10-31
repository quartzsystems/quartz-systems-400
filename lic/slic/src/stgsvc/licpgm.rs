// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Licensed programs in the service (`docs/LICPGM.md` §9): the catalog
//! seeded every IPL, the product store over the open store, the system
//! as the product functions see it, and the license public key.

use crate::ipl::system::{System, now_unix};

/// Seed the catalog's products; whether any was made.
pub fn seed(system: &mut System) -> bool {
    let Some(s) = system.storage.as_mut() else {
        return false;
    };
    let Some(qsys) = s.qsys else {
        return false;
    };
    match sec::guard::as_system(|| lpstore::seed(&mut s.store, qsys, now_unix())) {
        Ok(made) => made,
        Err(e) => {
            eprintln!("qsstg: licensed programs: {e}");
            false
        }
    }
}
