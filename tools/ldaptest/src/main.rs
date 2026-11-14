// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldaptest`: see `ldaptest --help` or `tools/ldaptest/README.md`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).ok();
    let code = ldaptest::cli::run(&args, &env, &mut std::io::stdout());
    std::process::exit(code);
}
