// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the writer writes. The generated code is compiled and run by
//! `net/nfs`'s tests over its runtime.

use xdrgen::{Options, compile};

const SRC: &str = "
const MAXNAME = 255;
enum stat { OK = 0, NOENT = 2 };
typedef string name<MAXNAME>;
typedef unsigned hyper id;
typedef opaque verf[8];
struct entry { id fileid; name nm; entry *next; };
struct dir { entry *entries; bool eof; };
struct attr { int type; verf v; };
union res switch (stat status) { case OK: dir ok; default: void; };
union mayattr switch (bool follows) { case TRUE: attr a; case FALSE: void; };
program DEMO { version DEMO_V1 { void DEMO_NULL(void) = 0; res DEMO_READ(name) = 1; } = 1; } = 400000;
";

fn out() -> String {
    compile(SRC, "demo.x", &Options::default()).unwrap()
}

#[test]
fn types() {
    let o = out();
    assert!(o.contains("pub const MAXNAME: u32 = 255;"));
    assert!(o.contains("pub enum stat {"));
    assert!(o.contains("pub struct name(pub Vec<u8>);"));
    assert!(o.contains("e.put_opaque(&self.0, 255)?;"));
    assert!(o.contains("pub type id = u64;"));
    assert!(o.contains("pub type verf = [u8; 8];"));
    assert!(o.contains("pub r#type: i32,"));
    assert!(o.contains("pub entries: Vec<entry>,"));
    assert!(o.contains("e.put_list(&self.entries)?;"));
    assert!(o.contains("pub type mayattr = Option<attr>;"));
    assert!(o.contains("default(stat),"));
    assert!(o.contains("ty: \"stat\""));
    // A list node is written without its link.
    let entry = o
        .split("pub struct entry {")
        .nth(1)
        .unwrap()
        .split('}')
        .next()
        .unwrap();
    assert!(!entry.contains("next"));
}

#[test]
fn programs() {
    let o = out();
    assert!(o.contains("pub const DEMO: u32 = 400000;"));
    assert!(o.contains("pub const DEMO_READ: u32 = 1;"));
    assert!(o.contains("pub mod DEMO {"));
    assert!(o.contains("pub mod DEMO_V1 {"));
    assert!(o.contains("DEMO_READ(super::super::name),"));
    assert!(o.contains("DEMO_READ(super::super::res),"));
    assert!(o.contains("impl crate::xdr::ProcBody for Args {"));
    assert!(o.contains("(1, \"DEMO_READ\"),"));
}

#[test]
fn the_runtime_path_is_the_callers() {
    let o = compile(
        SRC,
        "demo.x",
        &Options {
            runtime: "::rt".into(),
        },
    )
    .unwrap();
    assert!(o.contains("impl ::rt::Xdr for stat {"));
    assert!(!o.contains("crate::xdr"));
}

#[test]
fn errors_name_the_file() {
    let e = compile("struct a { b x; };", "bad.x", &Options::default()).unwrap_err();
    assert_eq!(e.to_string(), "bad.x:1:12: unknown type `b`");
}
