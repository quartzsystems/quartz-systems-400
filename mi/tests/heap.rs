// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Heap storage from IR and from the job API, and user spaces mapped
//! into the job.

#![allow(clippy::many_single_char_names)]

use mi::{Cond, MemUserSpaces, Program};

#[test]
fn alloc_realloc_dealloc_from_ir() {
    let mut p = Program::from_text(
        "module HEAP
static $sum: i32
static $after: i32
static $bad: i32
proc HEAP() entry {
L0:
  %p = alloc 8
  store [%p]:i32, 40
  store [%p +4]:i32, 2
  %q = realloc %p, 16
  store [%q +12]:i32, 100
  %a = load [%q]:i32
  %b = load [%q +4]:i32
  %c = load [%q +12]:i32
  %s = add %a, %b
  %s2 = add %s, %c
  store $sum, %s2
  monitor OLD, MCH3402 -> DEAD
  %z = load [%p]:i32
  store $after, 1
  endmon OLD
  br NEXT
DEAD:
  store $after, 2
  br NEXT
NEXT:
  dealloc %q
  monitor FREED, MCH3402 -> GONE
  %w = load [%q]:i32
  store $bad, 1
  endmon FREED
  ret
GONE:
  monitor NOTHEAP, MCH3601 -> NH
  %st = addr $sum
  dealloc %st
  store $bad, 2
  endmon NOTHEAP
  ret
NH:
  store $bad, 3
  ret
}
",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    p.call_entry().unwrap();
    let int =
        |name: &str| i32::from_be_bytes(p.static_bytes(name).unwrap()[..4].try_into().unwrap());
    assert_eq!(int("sum"), 142);
    assert_eq!(int("after"), 2, "the pointer from before realloc is dead");
    assert_eq!(
        int("bad"),
        3,
        "a freed block is dead; a static is not heap storage"
    );
    let j = p.job_ref();
    assert!(!j.pending());
    assert_eq!(
        j.handled.iter().map(|e| e.cond).collect::<Vec<_>>(),
        vec![Cond::Mch3402, Cond::Mch3402, Cond::Mch3601]
    );
}

#[test]
fn user_heaps_and_group_reclaim() {
    let mut p = Program::from_text("module H2\nproc H2() entry {\nL0:\n  ret\n}\n").unwrap();
    let j = p.job();
    let h = j.create_heap();
    assert_eq!(h, 1);
    let a = j.alloc_from(h, 100).unwrap();
    let b = j.alloc(50).unwrap();
    assert!(j.spaces.deref(a).is_ok());
    assert_eq!(j.alloc_from(9, 1), Err(Cond::Mch3601));
    j.discard_heap(h).unwrap();
    assert_eq!(j.spaces.deref(a), Err(Cond::Mch3402));
    assert!(j.spaces.deref(b).is_ok());
    assert_eq!(j.discard_heap(0), Err(Cond::Mch3601));
    assert_eq!(j.dealloc(a), Err(Cond::Mch3402));
    j.dealloc(b).unwrap();
    assert_eq!(j.dealloc(b), Err(Cond::Mch3402));
    // A named group's heap storage dies with the group.
    let g = j.act.named_group("GRP");
    j.act.current = g;
    let c = j.alloc(8).unwrap();
    j.act.current = 0;
    j.rclactgrp("GRP").unwrap();
    assert_eq!(j.spaces.deref(c), Err(Cond::Mch3402));
}

#[test]
fn user_spaces_map_sync_and_die() {
    let mut p = Program::from_text("module US\nproc US() entry {\nL0:\n  ret\n}\n").unwrap();
    let j = p.job();
    let mut src = MemUserSpaces::default();
    src.spaces
        .push(("QGPL".into(), "MYSPC".into(), vec![1, 2, 3, 4]));
    j.usrspcs.set_source(Box::new(src));
    let ptr = j.map_user_space("MYSPC", "QGPL").unwrap();
    let again = j.map_user_space("MYSPC", "QGPL").unwrap();
    assert_eq!(ptr.space, again.space, "mapped once");
    let a = j.spaces.deref(ptr).unwrap();
    assert_eq!(j.spaces.add(ptr, 4).unwrap().offset, 4);
    assert_eq!(j.spaces.add(ptr, 5), Err(Cond::Mch0601));
    // Write through the mapping, then sync.
    // SAFETY: the mapped copy is live and 4 bytes long.
    unsafe { *(a as *mut u8) = 9 };
    j.sync_user_spaces();
    assert_eq!(j.usrspcs.mapped("MYSPC", "QGPL"), Some(&[9u8, 2, 3, 4][..]));
    assert_eq!(j.map_user_space("NOPE", "QGPL"), Err(Cond::Mch3401));
    assert!(j.usrspcs.unmap("MYSPC", "QGPL", &mut j.spaces));
    assert_eq!(
        j.spaces.deref(ptr),
        Err(Cond::Mch3402),
        "deleted while mapped"
    );
}
