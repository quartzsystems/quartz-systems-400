// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Generates the protocol types from `x/*.x` into `OUT_DIR` with xdrgen.

use std::path::PathBuf;

const DESCRIPTIONS: &[&str] = &["rpc", "rpcb", "mount", "nfs3", "nlm", "nsm", "nfs4"];

fn main() {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let opts = xdrgen::Options {
        runtime: "crate::xdr".to_string(),
    };
    for name in DESCRIPTIONS {
        let src = PathBuf::from("x").join(format!("{name}.x"));
        println!("cargo:rerun-if-changed={}", src.display());
        if let Err(e) = xdrgen::compile_file(&src, &out.join(format!("{name}.rs")), &opts) {
            panic!("{e}");
        }
    }
}
