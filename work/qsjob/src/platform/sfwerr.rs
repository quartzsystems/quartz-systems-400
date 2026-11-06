// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A panic in the job is a software error of the system: the hook
//! leaves `stg::software_error_path` with what failed before the
//! process ends, and the service logs the problem when the job's
//! connection closes (`QSFWERRLOG`).

/// Install the hook for job `number`, keeping the default report.
pub fn install(number: u32) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = stg::software_error_path(number);
        if let Some(dir) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, info.to_string());
        default(info);
    }));
}
