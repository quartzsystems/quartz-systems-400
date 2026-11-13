// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfstest`: see `nfstest --help` or `tools/nfstest/README.md`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = nfstest::cli::run(&args, &mut std::io::stdout());
    std::process::exit(code);
}
