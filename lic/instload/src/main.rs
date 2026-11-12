// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/init` of the media UKI (see the library).

fn main() {
    #[cfg(target_os = "linux")]
    instload::load::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!("instload: runs as PID 1 of the QS/400 media UKI only");
}
