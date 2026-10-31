// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The APIs called from IR programs with IBM's parameter layouts:
//! messages, user spaces and indexes, object lists, job and system
//! information, date conversion, data queues and areas, commands, the
//! error code conventions and the `QLEAWI` procedures.

#![allow(
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]

use std::cell::RefCell;
use std::rc::Rc;

use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::exec::Program;
use mi::{Job, MemSource, MemUserSpaces};
use sysapi::{DataArea, DataAreaType, FieldInfo, FormatInfo, MemProvider, MemberInfo, ObjInfo};

/// Error code with 16 bytes provided.
const ERRC: &str = "static $err: char(16) inz(x\"00000010000000000000000000000000\")";

fn run_with(text: &str, provider: &Rc<RefCell<MemProvider>>) -> Program {
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    p.job()
        .usrspcs
        .set_source(Box::new(MemUserSpaces::default()));
    sysapi::install(p.job(), provider.clone());
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn run(text: &str) -> (Program, Rc<RefCell<MemProvider>>) {
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    let p = run_with(text, &provider);
    (p, provider)
}

fn bytes(p: &Program, name: &str) -> Vec<u8> {
    p.static_bytes(name).unwrap_or_else(|| panic!("no {name}"))
}

fn text(p: &Program, name: &str) -> String {
    ccsid::decode_trimmed(&bytes(p, name))
}

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

fn errc_ok(p: &Program) {
    let e = bytes(p, "err");
    assert_eq!(bin4(&e, 4), 0, "bytes available: {}", chars(&e, 8, 7));
}

#[test]
fn program_messages_are_sent_received_and_removed() {
    let (p, _) = run(&format!(
        "
module MSGS
extern sysapi QMHSNDPM($id: char(7), $f: char(20), $d: char(50), $l: i32, $t: char(10), $e: char(10), $c: i32, $k: char(4), $err: char(16))
extern sysapi QMHRCVPM($r: char(300), $rl: i32, $fmt: char(8), $e: char(10), $c: i32, $t: char(10), $k: char(4), $w: i32, $act: char(10), $err: char(16))
extern sysapi QMHRMVPM($e: char(10), $c: i32, $k: char(4), $which: char(10), $err: char(16))
static $id: char(7) inz(\"CPF9898\")
static $msgf: char(20) inz(\"QCPFMSG   QSYS\")
static $data: char(50) inz(\"hello from the test\")
static $len: i32 inz(19)
static $type: char(10) inz(\"*INFO\")
static $entry: char(10) inz(\"*\")
static $zero: i32 inz(0)
static $key: char(4)
{ERRC}
static $recv: char(300)
static $rlen: i32 inz(300)
static $fmt: char(8) inz(\"RCVM0100\")
static $any: char(10) inz(\"*ANY\")
static $wait: i32 inz(0)
static $same: char(10) inz(\"*SAME\")
static $remove: char(10) inz(\"*REMOVE\")
static $all: char(10) inz(\"*ALL\")
static $recv2: char(300)
proc MSGS() entry {{
L0:
  sysapi QMHSNDPM($id, $msgf, $data, $len, $type, $entry, $zero, $key, $err)
  sysapi QMHRCVPM($recv, $rlen, $fmt, $entry, $zero, $any, $key, $wait, $same, $err)
  sysapi QMHRMVPM($entry, $zero, $key, $all, $err)
  sysapi QMHRCVPM($recv2, $rlen, $fmt, $entry, $zero, $any, $key, $wait, $remove, $err)
  ret
}}
"
    ));
    errc_ok(&p);
    let r = bytes(&p, "recv");
    assert_eq!(chars(&r, 12, 7), "CPF9898");
    assert_eq!(chars(&r, 19, 2), "04", "an informational message");
    assert_eq!(
        &r[21..25],
        &bytes(&p, "key")[..],
        "the key the send returned"
    );
    assert_eq!(bin4(&r, 40), 19, "data length");
    let text_len = usize::try_from(bin4(&r, 48)).unwrap();
    let msg = chars(&r, 56 + 19, text_len);
    assert!(msg.contains("hello from the test"), "{msg}");
    assert!(bin4(&r, 0) > 75 && bin4(&r, 4) == bin4(&r, 0));
    let r2 = bytes(&p, "recv2");
    assert_eq!(bin4(&r2, 0), 56, "an empty receive after the remove");
    assert_eq!(chars(&r2, 12, 7), "");
}

#[test]
fn user_spaces_are_created_changed_mapped_and_deleted() {
    let (mut p, _) = run(&format!(
        "
module US
extern sysapi QUSCRTUS($n: char(20), $a: char(10), $s: i32, $i: char(1), $au: char(10), $t: char(50), $r: char(10), $err: char(16))
extern sysapi QUSCHGUS($n: char(20), $st: i32, $l: i32, $d: char(5), $f: char(1), $err: char(16))
extern sysapi QUSRTVUS($n: char(20), $st: i32, $l: i32, $d: char(5), $err: char(16))
extern sysapi QUSRUSAT($r: char(24), $rl: i32, $fmt: char(8), $n: char(20), $err: char(16))
extern sysapi QUSPTRUS($n: char(20), $p: ptr, $err: char(16))
extern sysapi QUSDLTUS($n: char(20), $err: char(16))
static $name: char(20) inz(\"SPACE     *CURLIB\")
static $attr: char(10) inz(\"TEST\")
static $size: i32 inz(100)
static $init: char(1) inz(x\"00\")
static $auth: char(10) inz(\"*ALL\")
static $text: char(50) inz(\"a test space\")
static $no: char(10) inz(\"*NO\")
{ERRC}
static $five: i32 inz(5)
static $len: i32 inz(5)
static $data: char(5) inz(\"HELLO\")
static $force: char(1) inz(\"0\")
static $got: char(5)
static $attrs: char(24)
static $alen: i32 inz(24)
static $fmt: char(8) inz(\"SPCA0100\")
static $ptr: ptr
static $byte: i8
static $after: char(5)
proc US() entry {{
L0:
  sysapi QUSCRTUS($name, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QUSCHGUS($name, $five, $len, $data, $force, $err)
  sysapi QUSRTVUS($name, $five, $len, $got, $err)
  sysapi QUSRUSAT($attrs, $alen, $fmt, $name, $err)
  sysapi QUSPTRUS($name, $ptr, $err)
  %p = load $ptr
  %b = load [%p +4]:i8
  store $byte, %b
  sysapi QUSDLTUS($name, $err)
  sysapi QUSRTVUS($name, $five, $len, $after, $err)
  ret
}}
"
    ));
    assert_eq!(text(&p, "got"), "HELLO");
    let a = bytes(&p, "attrs");
    assert_eq!(bin4(&a, 0), 24);
    assert_eq!(bin4(&a, 8), 100, "size");
    assert_eq!(chars(&a, 14, 10), "QGPL", "*CURLIB resolved");
    assert_eq!(
        bytes(&p, "byte"),
        vec![0xC8],
        "the pointer reads the H at offset 4"
    );
    let e = bytes(&p, "err");
    assert_eq!(chars(&e, 8, 7), "CPF9801", "retrieving after the delete");
    assert_eq!(bin4(&e, 4), 36, "bytes available: header and two names");
    let j = p.job();
    assert!(
        j.usrspcs.mapped("SPACE", "QGPL").is_none(),
        "unmapped by the delete"
    );
    assert!(!j.pending());
}

#[test]
fn object_lists_use_the_generic_header() {
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    for (name, ty, attr) in [
        ("PGMA", "*PGM", "RPGLE"),
        ("FILEA", "*FILE", "PF"),
        ("PGMB", "*PGM", "CLLE"),
    ] {
        provider.borrow_mut().objects.push(ObjInfo {
            name: name.into(),
            lib: "QGPL".into(),
            ty: ty.into(),
            attr: attr.into(),
            text: format!("object {name}"),
            owner: "QSECOFR".into(),
            created: "1261021120000".into(),
            changed: "1261021120000".into(),
            source: None,
            user_attr: String::new(),
        });
    }
    let mut p = run_with(
        &format!(
            "
module OBJ
extern sysapi QUSCRTUS($n: char(20), $a: char(10), $s: i32, $i: char(1), $au: char(10), $t: char(50), $r: char(10), $err: char(16))
extern sysapi QUSLOBJ($us: char(20), $fmt: char(8), $obj: char(20), $ty: char(10), $err: char(16))
extern sysapi QUSROBJD($r: char(180), $rl: i32, $fmt: char(8), $obj: char(20), $ty: char(10), $err: char(16))
static $us: char(20) inz(\"LIST      QGPL\")
static $attr: char(10) inz(\"USRSPC\")
static $size: i32 inz(16)
static $init: char(1) inz(x\"00\")
static $auth: char(10) inz(\"*ALL\")
static $text: char(50)
static $no: char(10) inz(\"*NO\")
{ERRC}
static $fmt: char(8) inz(\"OBJL0200\")
static $obj: char(20) inz(\"PGM*      QGPL\")
static $ty: char(10) inz(\"*PGM\")
static $recv: char(180)
static $rlen: i32 inz(180)
static $dfmt: char(8) inz(\"OBJD0200\")
static $one: char(20) inz(\"FILEA     *LIBL\")
static $any: char(10) inz(\"*ALL\")
proc OBJ() entry {{
L0:
  sysapi QUSCRTUS($us, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QUSLOBJ($us, $fmt, $obj, $ty, $err)
  sysapi QUSROBJD($recv, $rlen, $dfmt, $one, $any, $err)
  ret
}}
"
        ),
        &provider,
    );
    errc_ok(&p);
    let r = bytes(&p, "recv");
    assert_eq!(bin4(&r, 0), 180);
    assert_eq!(chars(&r, 8, 10), "FILEA");
    assert_eq!(chars(&r, 28, 10), "*FILE");
    assert_eq!(chars(&r, 90, 10), "PF");
    assert_eq!(chars(&r, 100, 50), "object FILEA");
    let j = p.job();
    let us = j
        .usrspcs
        .mapped("LIST", "QGPL")
        .expect("the list space")
        .to_vec();
    assert_eq!(bin4(&us, 64), 192, "generic header size");
    assert_eq!(chars(&us, 72, 8), "OBJL0200");
    assert_eq!(chars(&us, 80, 10), "QUSLOBJ");
    assert_eq!(chars(&us, 103, 1), "C", "complete");
    let n = usize::try_from(bin4(&us, 132)).unwrap();
    let off = usize::try_from(bin4(&us, 124)).unwrap();
    let size = usize::try_from(bin4(&us, 136)).unwrap();
    assert_eq!((n, size), (2, 108), "two PGM* programs");
    let e1 = &us[off..off + size];
    let e2 = &us[off + size..off + 2 * size];
    assert_eq!(chars(e1, 0, 10), "PGMA");
    assert_eq!(chars(e1, 31, 10), "RPGLE");
    assert_eq!(chars(e2, 0, 10), "PGMB");
    assert_eq!(chars(e2, 41, 50), "object PGMB");
    assert_eq!(
        usize::try_from(bin4(&us, 104)).unwrap(),
        off + 2 * size,
        "space used"
    );
}

#[test]
fn job_system_values_release_and_dates() {
    let (p, _) = run(&format!(
        "
module JOB
extern sysapi QUSRJOBI($r: char(86), $rl: i32, $fmt: char(8), $job: char(26), $iid: char(16), $err: char(16))
extern sysapi QWCRSVAL($r: char(200), $rl: i32, $n: i32, $names: char(20), $err: char(16))
extern sysapi QSZRTVPR($r: char(108), $rl: i32, $fmt: char(8), $prd: char(27), $err: char(16))
extern sysapi QWCCVTDT($if: char(10), $i: char(17), $of: char(10), $o: char(17), $err: char(16))
static $ji: char(86)
static $jl: i32 inz(86)
static $jfmt: char(8) inz(\"JOBI0100\")
static $job: char(26) inz(\"*\")
static $iid: char(16)
{ERRC}
static $sv: char(200)
static $svl: i32 inz(200)
static $two: i32 inz(2)
static $names: char(20) inz(\"QDATFMT   QCCSID\")
static $pr: char(108)
static $prl: i32 inz(108)
static $pfmt: char(8) inz(\"PRDR0100\")
static $prd: char(27) inz(\"*OPSYS *CUR  0000*CODE\")
static $yymd: char(10) inz(\"*YYMD\")
static $in: char(17) inz(\"20261021130509250\")
static $mdyy: char(10) inz(\"*MDYY\")
static $out: char(17)
static $dts: char(10) inz(\"*DTS\")
static $stamp: char(17)
static $back: char(17)
proc JOB() entry {{
L0:
  sysapi QUSRJOBI($ji, $jl, $jfmt, $job, $iid, $err)
  sysapi QWCRSVAL($sv, $svl, $two, $names, $err)
  sysapi QSZRTVPR($pr, $prl, $pfmt, $prd, $err)
  sysapi QWCCVTDT($yymd, $in, $mdyy, $out, $err)
  sysapi QWCCVTDT($yymd, $in, $dts, $stamp, $err)
  sysapi QWCCVTDT($dts, $stamp, $yymd, $back, $err)
  ret
}}
"
    ));
    errc_ok(&p);
    let ji = bytes(&p, "ji");
    assert_eq!(bin4(&ji, 0), 86);
    assert_eq!(chars(&ji, 8, 10), "QSVCJOB");
    assert_eq!(chars(&ji, 18, 10), "QSYS");
    assert_eq!(chars(&ji, 28, 6), "000001");
    assert_eq!(chars(&ji, 50, 10), "*ACTIVE");
    let sv = bytes(&p, "sv");
    assert_eq!(bin4(&sv, 0), 2);
    let o1 = usize::try_from(bin4(&sv, 4)).unwrap();
    let o2 = usize::try_from(bin4(&sv, 8)).unwrap();
    assert_eq!(chars(&sv, o1, 10), "QDATFMT");
    assert_eq!(chars(&sv, o1 + 10, 1), "C");
    assert_eq!(chars(&sv, o2, 10), "QCCSID");
    assert_eq!(chars(&sv, o2 + 10, 1), "B");
    assert_eq!(bin4(&sv, o2 + 16), 37);
    let pr = bytes(&p, "pr");
    assert_eq!(chars(&pr, 12, 7), "QS00SS1");
    assert_eq!(chars(&pr, 63, 2), "90");
    assert_eq!(chars(&pr, 19, 6), "V1R0M0");
    assert_eq!(text(&p, "out"), "10212026130509250");
    assert_eq!(text(&p, "back"), "20261021130509250", "*DTS round trip");
    assert_ne!(bytes(&p, "stamp")[..8], [0; 8]);
}

#[test]
fn data_queues_and_data_areas() {
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    provider.borrow_mut().add_dtaq("QGPL", "TESTQ", 100, 0, 'F');
    let (p, prov) = (
        run_with(
            "
module DQ
extern sysapi QSNDDTAQ($n: char(10), $l: char(10), $len: packed(5,0), $d: char(5))
extern sysapi QRCVDTAQ($n: char(10), $l: char(10), $len: packed(5,0), $d: char(10), $w: packed(5,0))
extern sysapi QMHQRDQD($r: char(100), $rl: i32, $fmt: char(8), $q: char(20), $err: char(16))
extern sysapi QXXCHGDA($da: char(20), $s: i32, $l: i32, $v: char(5))
extern sysapi QXXRTVDA($r: char(5), $rl: i32, $da: char(20), $s: i32, $l: i32)
extern sysapi QWCRDTAA($r: char(60), $rl: i32, $da: char(20), $s: i32, $l: i32, $err: char(16))
static $qn: char(10) inz(\"TESTQ\")
static $ql: char(10) inz(\"QGPL\")
static $slen: packed(5,0) inz(5)
static $sdata: char(5) inz(\"ABCDE\")
static $rlen: packed(5,0)
static $rdata: char(10)
static $wait: packed(5,0) inz(0)
static $desc: char(100)
static $dlen: i32 inz(100)
static $dfmt: char(8) inz(\"RDQD0100\")
static $qual: char(20) inz(\"TESTQ     *LIBL\")
static $err: char(16) inz(x\"00000010000000000000000000000000\")
static $lda: char(20) inz(\"*LDA      *LIBL\")
static $one: i32 inz(1)
static $five: i32 inz(5)
static $val: char(5) inz(\"HELLO\")
static $got: char(5)
static $area: char(60)
static $alen: i32 inz(60)
proc DQ() entry {
L0:
  sysapi QSNDDTAQ($qn, $ql, $slen, $sdata)
  sysapi QRCVDTAQ($qn, $ql, $rlen, $rdata, $wait)
  sysapi QMHQRDQD($desc, $dlen, $dfmt, $qual, $err)
  sysapi QXXCHGDA($lda, $one, $five, $val)
  sysapi QXXRTVDA($got, $five, $lda, $one, $five)
  sysapi QWCRDTAA($area, $alen, $lda, $one, $five, $err)
  ret
}
",
            &provider,
        ),
        provider,
    );
    errc_ok(&p);
    assert_eq!(
        bytes(&p, "rlen"),
        vec![0x00, 0x00, 0x5C],
        "5 in packed(5,0)"
    );
    assert_eq!(text(&p, "rdata"), "ABCDE");
    let d = bytes(&p, "desc");
    assert_eq!(bin4(&d, 8), 100, "maximum entry length");
    assert_eq!(chars(&d, 16, 1), "F");
    assert_eq!(bin4(&d, 72), 0, "received, so empty");
    assert_eq!(chars(&d, 80, 10), "TESTQ");
    assert_eq!(text(&p, "got"), "HELLO");
    let a = bytes(&p, "area");
    assert_eq!(chars(&a, 8, 10), "*CHAR");
    assert_eq!(bin4(&a, 28), 1024);
    assert_eq!(chars(&a, 36, 5), "HELLO");
    let lda = prov.borrow_mut().dtaaras[&("*LIBL".to_string(), "*LDA".to_string())].clone();
    assert_eq!(ccsid::decode_trimmed(&lda.value[..5]), "HELLO");
    let _ = DataArea {
        ty: DataAreaType::Char,
        lib: String::new(),
        len: 0,
        decimals: 0,
        value: Vec::new(),
    };
}

#[test]
fn commands_escapes_and_error_code_formats() {
    let (p, prov) = run("
module CMD
extern sysapi QCMDEXC($c: char(30), $l: packed(15,5))
extern sysapi QUSRTVUS($n: char(20), $st: i32, $l: i32, $d: char(5), $err: char(40))
static $cmd: char(30) inz(\"DSPLIB QGPL\")
static $clen: packed(15,5) inz(11)
static $name: char(20) inz(\"NOSUCH    QGPL\")
static $one: i32 inz(1)
static $got: char(5)
static $err0: char(40) inz(x\"00000000\")
static $err2: char(40) inz(x\"FFFFFFFF00000028\")
proc CMD() entry {
L0:
  sysapi QCMDEXC($cmd, $clen)
  monitor M, CPF9801 -> GOT
  sysapi QUSRTVUS($name, $one, $one, $got, $err0)
  dsply \"not reached\"
  endmon M
  ret
GOT:
  dsply \"escaped\"
  sysapi QUSRTVUS($name, $one, $one, $got, $err2)
  ret
}
");
    assert_eq!(prov.borrow().commands, vec!["DSPLIB QGPL"]);
    let j = p.job_ref();
    assert_eq!(j.console, vec!["escaped"]);
    assert!(!j.pending());
    let e = bytes(&p, "err2");
    assert_eq!(bin4(&e, 0), -1, "ERRC0200 keeps its key");
    assert_eq!(bin4(&e, 4), 40);
    assert_eq!(bin4(&e, 8), 52, "bytes available");
    assert_eq!(chars(&e, 12, 7), "CPF9801");
    assert_eq!(bin4(&e, 20), 37);
    assert_eq!(bin4(&e, 24), 32);
    assert_eq!(bin4(&e, 28), 20);
    assert_eq!(
        chars(&e, 32, 8),
        "NOSUCH",
        "the data, truncated at the room"
    );
}

#[test]
fn user_indexes_hold_keyed_entries() {
    let (p, _) = run(&format!(
        "
module UI
extern sysapi QUSCRTUI($n: char(20), $a: char(10), $elt: char(1), $el: i32, $ki: char(1), $kl: i32, $im: char(1), $op: char(1), $au: char(10), $t: char(50), $r: char(10), $err: char(16))
extern sysapi QUSADDUI($n: char(20), $ret: i32, $lens: i32, $ins: char(1), $e: char(30), $el: i32, $cnt: i32, $err: char(16))
extern sysapi QUSRTVUI($r: char(20), $rl: i32, $lens: char(8), $ll: i32, $cnt: i32, $n: char(20), $rule: i32, $k: char(3), $kl: i32, $err: char(16))
extern sysapi QUSRUIAT($r: char(92), $rl: i32, $fmt: char(8), $n: char(20), $err: char(16))
static $name: char(20) inz(\"IDX       QGPL\")
static $attr: char(10)
static $fixed: char(1) inz(\"F\")
static $elen: i32 inz(10)
static $keyed: char(1) inz(\"1\")
static $klen: i32 inz(3)
static $imm: char(1) inz(\"1\")
static $opt: char(1) inz(\"0\")
static $auth: char(10) inz(\"*ALL\")
static $text: char(50) inz(\"index\")
static $no: char(10) inz(\"*NO\")
{ERRC}
static $added: i32
static $lens: i32
static $insert: char(1) inz(\"1\")
static $entries: char(30) inz(\"CCC3......AAA1......BBB2......\")
static $three: i32 inz(3)
static $recv: char(20)
static $rlen: i32 inz(20)
static $rlens: char(8)
static $rll: i32 inz(8)
static $count: i32
static $rule: i32 inz(4)
static $key: char(3) inz(\"BBB\")
static $ia: char(92)
static $ial: i32 inz(92)
static $ifmt: char(8) inz(\"IDXA0100\")
proc UI() entry {{
L0:
  sysapi QUSCRTUI($name, $attr, $fixed, $elen, $keyed, $klen, $imm, $opt, $auth, $text, $no, $err)
  sysapi QUSADDUI($name, $added, $lens, $insert, $entries, $elen, $three, $err)
  sysapi QUSRTVUI($recv, $rlen, $rlens, $rll, $count, $name, $rule, $key, $klen, $err)
  sysapi QUSRUIAT($ia, $ial, $ifmt, $name, $err)
  ret
}}
"
    ));
    errc_ok(&p);
    assert_eq!(bin4(&bytes(&p, "added"), 0), 3);
    assert_eq!(
        bin4(&bytes(&p, "count"), 0),
        2,
        "BBB and CCC are not less than BBB"
    );
    assert_eq!(text(&p, "recv"), "BBB2......CCC3......", "in key order");
    assert_eq!(bytes(&p, "rlens"), vec![0, 0, 0, 10, 0, 0, 0, 10]);
    let ia = bytes(&p, "ia");
    assert_eq!(chars(&ia, 12, 1), "F");
    assert_eq!(bin4(&ia, 13), 10);
    assert_eq!(bin4(&ia, 18), 3);
    assert_eq!(bin4(&ia, 24), 3, "entries");
    assert_eq!(chars(&ia, 32, 10), "QGPL");
    assert_eq!(chars(&ia, 42, 50), "index");
}

#[test]
fn file_descriptions_list_members_and_fields() {
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    provider.borrow_mut().files.insert(
        ("QGPL".into(), "CUST".into()),
        sysapi::MemFile {
            info: sysapi::FileInfo {
                name: "CUST".into(),
                lib: "QGPL".into(),
                attr: "PF".into(),
                text: "customers".into(),
                rec_len: 35,
                keyed: true,
                source: false,
                based_on: Vec::new(),
            },
            members: vec![
                MemberInfo {
                    name: "CUST".into(),
                    srctype: String::new(),
                    created: "1261021120000".into(),
                    changed: "1261021120000".into(),
                    text: "first".into(),
                    records: 12,
                    deleted: 1,
                },
                MemberInfo {
                    name: "OLD".into(),
                    ..MemberInfo::default()
                },
            ],
            formats: vec![FormatInfo {
                name: "CUSTR".into(),
                rec_len: 35,
                text: "customer record".into(),
                fields: vec![
                    FieldInfo {
                        name: "CUSTNO".into(),
                        ty: 'P',
                        pos: 1,
                        len: 4,
                        digits: 7,
                        decimals: 0,
                        text: "number".into(),
                        headings: ["Cust".into(), "No".into(), String::new()],
                    },
                    FieldInfo {
                        name: "NAME".into(),
                        ty: 'A',
                        pos: 5,
                        len: 30,
                        digits: 0,
                        decimals: 0,
                        text: "name".into(),
                        headings: [String::new(), String::new(), String::new()],
                    },
                ],
            }],
            dependents: vec![sysapi::DepInfo {
                name: "CUSTL1".into(),
                lib: "QGPL".into(),
                member: "CUSTL1".into(),
                kind: 'L',
            }],
        },
    );
    let mut p = run_with(
        &format!(
            "
module DB
extern sysapi QUSCRTUS($n: char(20), $a: char(10), $s: i32, $i: char(1), $au: char(10), $t: char(50), $r: char(10), $err: char(16))
extern sysapi QUSLMBR($us: char(20), $fmt: char(8), $f: char(20), $m: char(10), $o: char(1), $err: char(16))
extern sysapi QUSLFLD($us: char(20), $fmt: char(8), $f: char(20), $rf: char(10), $o: char(1), $err: char(16))
extern sysapi QUSRMBRD($r: char(200), $rl: i32, $fmt: char(8), $f: char(20), $m: char(10), $o: char(1), $err: char(16))
extern sysapi QDBLDBR($us: char(20), $fmt: char(8), $f: char(20), $rf: char(10), $m: char(10), $err: char(16))
static $us1: char(20) inz(\"MBRS      QGPL\")
static $us2: char(20) inz(\"FLDS      QGPL\")
static $us3: char(20) inz(\"DEPS      QGPL\")
static $attr: char(10)
static $size: i32 inz(16)
static $init: char(1) inz(x\"00\")
static $auth: char(10) inz(\"*ALL\")
static $text: char(50)
static $no: char(10) inz(\"*NO\")
{ERRC}
static $mfmt: char(8) inz(\"MBRL0200\")
static $file: char(20) inz(\"CUST      *LIBL\")
static $all: char(10) inz(\"*ALL\")
static $ovr: char(1) inz(\"0\")
static $ffmt: char(8) inz(\"FLDL0100\")
static $first: char(10) inz(\"*FIRST\")
static $recv: char(200)
static $rlen: i32 inz(200)
static $dfmt: char(8) inz(\"MBRD0200\")
static $bfmt: char(8) inz(\"DBRL0100\")
proc DB() entry {{
L0:
  sysapi QUSCRTUS($us1, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QUSCRTUS($us2, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QUSCRTUS($us3, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QUSLMBR($us1, $mfmt, $file, $all, $ovr, $err)
  sysapi QUSLFLD($us2, $ffmt, $file, $first, $ovr, $err)
  sysapi QUSRMBRD($recv, $rlen, $dfmt, $file, $first, $ovr, $err)
  sysapi QDBLDBR($us3, $bfmt, $file, $first, $all, $err)
  ret
}}
"
        ),
        &provider,
    );
    errc_ok(&p);
    let r = bytes(&p, "recv");
    assert_eq!(chars(&r, 8, 10), "CUST");
    assert_eq!(chars(&r, 18, 10), "QGPL");
    assert_eq!(chars(&r, 28, 10), "CUST");
    assert_eq!(chars(&r, 38, 10), "PF");
    assert_eq!(chars(&r, 84, 50), "first");
    assert_eq!(bin4(&r, 140), 12);
    assert_eq!(bin4(&r, 144), 1);
    let j = p.job();
    let m = j.usrspcs.mapped("MBRS", "QGPL").unwrap().to_vec();
    assert_eq!(bin4(&m, 132), 2);
    let off = usize::try_from(bin4(&m, 124)).unwrap();
    assert_eq!(chars(&m, off, 10), "CUST");
    assert_eq!(chars(&m, off + 46, 50), "first");
    assert_eq!(chars(&m, off + 100, 10), "OLD");
    let f = j.usrspcs.mapped("FLDS", "QGPL").unwrap().to_vec();
    assert_eq!((bin4(&f, 132), bin4(&f, 136)), (2, 266));
    let hoff = usize::try_from(bin4(&f, 116)).unwrap();
    assert_eq!(
        chars(&f, hoff + 30, 10),
        "CUSTR",
        "the format in the header section"
    );
    assert_eq!(bin4(&f, hoff + 40), 35);
    let off = usize::try_from(bin4(&f, 124)).unwrap();
    assert_eq!(chars(&f, off, 10), "CUSTNO");
    assert_eq!(chars(&f, off + 10, 1), "P");
    assert_eq!(bin4(&f, off + 12), 1);
    assert_eq!(bin4(&f, off + 20), 4);
    assert_eq!(bin4(&f, off + 24), 7);
    assert_eq!(chars(&f, off + 152, 20), "Cust");
    assert_eq!(chars(&f, off + 266, 10), "NAME");
    assert_eq!(bin4(&f, off + 266 + 262), 37, "CCSID of a character field");
    let d = j.usrspcs.mapped("DEPS", "QGPL").unwrap().to_vec();
    assert_eq!(bin4(&d, 132), 1);
    let off = usize::try_from(bin4(&d, 124)).unwrap();
    assert_eq!(chars(&d, off, 10), "CUSTL1");
    assert_eq!(chars(&d, off + 30, 1), "L");
}

#[test]
fn qleawi_dates_heaps_and_tests() {
    let (p, _) = run("
module CEE
extern proc CEEDAYS($d: char(12), $p: char(12), $l: i32, $fc: char(12) *NOPASS)
extern proc CEEDATE($l: i32, $p: char(13), $o: char(80), $fc: char(12) *NOPASS)
extern proc CEESECS($d: char(12), $p: char(12), $s: f64, $fc: char(12) *NOPASS)
extern proc CEEDATM($s: f64, $p: char(13), $o: char(80), $fc: char(12) *NOPASS)
extern proc CEEGPID($v: i32, $pl: i32, $fc: char(12) *NOPASS)
extern proc CEETSTA($pr: i32, $n: i32, $fc: char(12) *NOPASS)
extern proc CEECRHP($h: i32, $i: i32, $inc: i32, $s: i32, $fc: char(12) *NOPASS)
extern proc CEEGTST($h: i32, $sz: i32, $addr: ptr, $fc: char(12) *NOPASS)
extern proc CEEFRST($addr: ptr, $fc: char(12) *NOPASS)
extern proc CEEQCEN($c: i32, $fc: char(12) *NOPASS)
static $in: char(12) inz(x\"000AF2F0F2F660F1F060F2F1\")
static $pic: char(12) inz(x\"000AE8E8E8E860D4D460C4C4\")
static $pic2: char(13) inz(x\"000BC4C440D4949440E8E8E8E8\")
static $lil: i32
static $out: char(80)
static $secs: f64
static $out2: char(80)
static $ver: i32
static $plat: i32
static $fc: char(12)
static $pres1: i32
static $pres3: i32
static $two: i32 inz(2)
static $three: i32 inz(3)
static $heap: i32
static $zero: i32 inz(0)
static $size: i32 inz(32)
static $addr: ptr
static $got: i32
static $cen: i32
proc SUB($a: i32, $b: i32) {
L0:
  callb CEETSTA($pres1, $two, $fc)
  callb CEETSTA($pres3, $three, $fc)
  ret
}
proc CEE() entry {
L0:
  callb CEEDAYS($in, $pic, $lil, $fc)
  callb CEEDATE($lil, $pic2, $out, $fc)
  callb CEESECS($in, $pic, $secs, $fc)
  callb CEEDATM($secs, $pic2, $out2, $fc)
  callb CEEGPID($ver, $plat, $fc)
  callb SUB($ver, $plat)
  callb CEECRHP($heap, $zero, $zero, $zero, $fc)
  callb CEEGTST($heap, $size, $addr, $fc)
  %p = load $addr
  store [%p +8]:i32, 4242
  %v = load [%p +8]:i32
  store $got, %v
  callb CEEFRST($addr, $fc)
  callb CEEQCEN($cen, $fc)
  monitor M, *ALL -> FREED
  %q = load $addr
  %w = load [%q +8]:i32
  store $got, %w
  endmon M
  ret
FREED:
  dsply \"freed\"
  ret
}
");
    let lil = bin4(&bytes(&p, "lil"), 0);
    assert_eq!(
        lil,
        sysapi::picture::lilian_of(sysapi::picture::civil_of_lilian(lil)),
        "a Lilian day"
    );
    let expect = sysapi::picture::lilian_of(
        sysapi::picture::parse("2026-10-21", "YYYY-MM-DD", 80, 2026).unwrap(),
    );
    assert_eq!(lil, expect);
    assert_eq!(text(&p, "out"), "21 Oct 2026");
    assert_eq!(text(&p, "out2"), "21 Oct 2026");
    assert_eq!(bytes(&p, "fc"), vec![0; 12], "CEE0000");
    assert_eq!(bin4(&bytes(&p, "ver"), 0), 1);
    assert_eq!(bin4(&bytes(&p, "plat"), 0), 3);
    assert_eq!(bin4(&bytes(&p, "pres1"), 0), 1, "two arguments were passed");
    assert_eq!(bin4(&bytes(&p, "pres3"), 0), 0);
    assert!(bin4(&bytes(&p, "heap"), 0) >= 1);
    assert_eq!(bin4(&bytes(&p, "got"), 0), 4242);
    assert_eq!(bin4(&bytes(&p, "cen"), 0), 80);
    let j = p.job_ref();
    assert_eq!(j.console, vec!["freed"], "the freed block cannot be read");
    assert!(!j.pending());
}

#[test]
fn activation_group_exits_run_on_reclaim() {
    let text = "module AGX\nextern proc CEE4RAGE($p: procptr, $fc: char(12) *NOPASS)\nstatic $pp: procptr\nproc EXIT($mark: i32, $reason: i32, $result: i32, $urc: i32) {\nL0:\n  %r = load $reason\n  dsply %r\n  dsply \"exit ran\"\n  ret\n}\nproc AGX() entry {\nL0:\n  %p = procaddr EXIT\n  store $pp, %p\n  callb CEE4RAGE($pp)\n  dsply \"main\"\n  ret\n}\n";
    let m = ModuleObject::compile(qir::parse(text).unwrap(), CreateData::default(), 0).unwrap();
    let mut opts = BindOptions::new("AGX", PgmKind::Pgm);
    opts.attr.actgrp = ActGrpAttr::New;
    let (object, _) = ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .unwrap();
    let mut src = MemSource::default();
    src.add("QGPL", object);
    let mut job = Job::boxed(Box::new(src));
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    sysapi::install(&mut job, provider);
    job.call_program("AGX", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["main", "1", "exit ran"]);
    assert!(!job.pending());
    assert_eq!(sysapi::qleawi_exports().len(), 31);
    sysapi::uninstall(&job);
}
