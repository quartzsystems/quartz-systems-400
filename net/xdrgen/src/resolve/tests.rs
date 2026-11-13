// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::ir::{Disc, Kind, Model, Shape};
use super::resolve;
use crate::parse::parse;

fn model(s: &str) -> Model {
    resolve(&parse(s).unwrap()).unwrap()
}

fn err(s: &str) -> String {
    let e = resolve(&parse(s).unwrap()).unwrap_err();
    format!("{}: {}", e.pos, e.msg)
}

#[test]
fn forward_references_and_constants() {
    let m = model(
        "struct a { b x; opaque o<MAX>; };
         typedef int b;
         const MAX = OTHER;
         const OTHER = 0x20;",
    );
    let Kind::Struct(f) = &m.ty("a").unwrap().kind else {
        panic!()
    };
    assert_eq!(f[1].1, Shape::VarOpaque(32));
    assert_eq!(m.ty("a").unwrap().min, 8);
}

#[test]
fn lists_through_typedefs_and_directly() {
    let m = model(
        "typedef struct node *list;
         struct node { int v; list next; };
         struct entry { int v; entry *nextentry; };
         struct dir { entry *entries; bool eof; };
         program P { version V { list GET(void) = 1; } = 1; } = 9;",
    );
    let Kind::Struct(f) = &m.ty("node").unwrap().kind else {
        panic!()
    };
    assert_eq!(f.len(), 1);
    assert_eq!(
        m.ty("list").unwrap().kind,
        Kind::Newtype(Shape::List("node".into()))
    );
    let Kind::Struct(f) = &m.ty("dir").unwrap().kind else {
        panic!()
    };
    assert_eq!(f[0].1, Shape::List("entry".into()));
}

#[test]
fn a_node_used_directly_is_not_a_list() {
    let m = model(
        "struct tree { int v; tree *left; tree *right; };
         struct n { int v; n *next; };
         struct holder { n first; };",
    );
    let Kind::Struct(f) = &m.ty("tree").unwrap().kind else {
        panic!()
    };
    assert_eq!(
        f[1].1,
        Shape::Optional(Box::new(Shape::Named("tree".into())), true)
    );
    let Kind::Struct(f) = &m.ty("n").unwrap().kind else {
        panic!()
    };
    assert_eq!(f.len(), 2);
    assert_eq!(
        f[1].1,
        Shape::Optional(Box::new(Shape::Named("n".into())), true)
    );
}

#[test]
fn bool_union_is_an_option() {
    let m = model(
        "struct a { int x; }; union p switch (bool f) { case TRUE: a v; case FALSE: void; };",
    );
    assert_eq!(
        m.ty("p").unwrap().kind,
        Kind::Alias(Shape::Optional(Box::new(Shape::Named("a".into())), false))
    );
}

#[test]
fn union_labels() {
    let m = model(
        "enum e { A = 1, B = 2, C = 3 };
         typedef e e2;
         union u switch (e2 d) { case A: case B: int x; default: void; };
         union v switch (unsigned d) { case 0: void; case C: hyper h; };",
    );
    let Kind::Union {
        disc,
        arms,
        default,
    } = &m.ty("u").unwrap().kind
    else {
        panic!()
    };
    assert_eq!(*disc, Disc::Enum("e".into()));
    assert_eq!(arms.len(), 2);
    assert_eq!(*default, Some(None));
    let Kind::Union { arms, .. } = &m.ty("v").unwrap().kind else {
        panic!()
    };
    assert_eq!(arms[0].variant, "V0");
    assert_eq!(arms[1].variant, "C");
    assert_eq!(arms[1].value, 3);
}

#[test]
fn program_names_may_repeat_with_their_number() {
    let m = model(
        "program P { version V1 { void NUL(void) = 0; int GET(int) = 1; } = 1;
                     version V3 { void NUL(void) = 0; int GET(int) = 1; int MORE(int) = 20; } = 3; } = 100;
         const LAST = MORE;",
    );
    assert_eq!(m.programs[0].versions[1].procs.len(), 3);
    assert_eq!(m.consts, vec![("LAST".to_string(), 20)]);
}

#[test]
fn errors() {
    assert_eq!(err("struct a { b x; };"), "1:12: unknown type `b`");
    assert_eq!(
        err("typedef opaque o<NOPE>;"),
        "1:18: unknown constant `NOPE`"
    );
    assert!(err("const A = B; const B = A;").contains("defined by itself"));
    assert!(err("struct a { int x; }; struct a { int y; };").contains("defined twice"));
    assert!(err("const A = 1; enum e { A = 2 };").contains("defined twice"));
    assert!(err("struct a { int x; int x; };").contains("declared twice"));
    assert!(err("typedef opaque o<-1>;").contains("outside"));
    assert!(err("enum e { A = 0x100000000 };").contains("outside int"));
    assert!(err("enum e { A = 1, B = 1 };").contains("the value of"));
    assert!(err("struct a { a x; };").contains("contains itself"));
    assert!(err("struct a { b x; }; struct b { a y; };").contains("contains itself"));
    assert!(err("union u switch (hyper d) { case 0: void; };").contains("discriminant"));
    assert!(
        err("struct s { int x; }; union u switch (s d) { case 0: void; };")
            .contains("discriminant")
    );
    assert!(
        err("enum e { A = 1 }; union u switch (e d) { case 2: void; };").contains("not a value")
    );
    assert!(err("union u switch (int d) { case 1: void; case 1: int x; };").contains("twice"));
    assert!(err("union u switch (bool d) { case 2: void; };").contains("not a value"));
    assert!(err("typedef opaque z[0]; struct s { z a<>; };").contains("no bytes"));
    assert!(
        err("program P { version V { void A(void) = 1; void B(void) = 1; } = 1; } = 1;")
            .contains("given twice")
    );
    assert!(
        err("program P { version V { void A(void) = 1; } = 1; version W { void A(void) = 2; } = 2; } = 1;")
            .contains("different numbers")
    );
    assert!(
        err("program P { version V { nope A(void) = 1; } = 1; } = 1;").contains("unknown type")
    );
}
