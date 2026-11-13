// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The XDR description compiler (docs/NFS.md section 3): a description
//! in the XDR language (RFC 4506 section 6, with the RPC `program`
//! definitions of RFC 5531 section 12) is parsed into a model, its
//! names resolved, and written out as Rust types that encode and
//! decode themselves over a runtime (`net/nfs`'s `xdr` module).
//!
//! [`compile`] does all of it; [`parse::parse`], [`resolve::resolve`]
//! and [`rust::write`] are the stages. [`rust`] says what each
//! definition becomes.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod error;
pub mod lex;
pub mod model;
pub mod parse;
pub mod resolve;
pub mod rust;

pub use error::{Error, Pos};
pub use rust::Options;

/// The Rust source for the description `src`, named `file` in messages.
pub fn compile(src: &str, file: &str, opts: &Options) -> Result<String, Error> {
    let defs = parse::parse(src).map_err(|e| e.in_file(file))?;
    let model = resolve::resolve(&defs).map_err(|e| e.in_file(file))?;
    Ok(rust::write(&model, file, opts))
}

/// Compiles the description at `path` into `out`, for a build script.
pub fn compile_file(
    path: &std::path::Path,
    out: &std::path::Path,
    opts: &Options,
) -> Result<(), Error> {
    let file = path.file_name().map_or_else(
        || path.display().to_string(),
        |f| f.to_string_lossy().into_owned(),
    );
    let src = std::fs::read_to_string(path).map_err(|e| Error {
        file: file.clone(),
        pos: Pos::default(),
        msg: format!("cannot read: {e}"),
    })?;
    let rs = compile(&src, &file, opts)?;
    std::fs::write(out, rs).map_err(|e| Error {
        file,
        pos: Pos::default(),
        msg: format!("cannot write {}: {e}", out.display()),
    })
}
