// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Robustness: the parser and the expansion of what it accepts never
//! panic, whatever the input, and deep nesting ends in an error rather
//! than a stack overflow on an ordinary 2 MB thread.

mod common;

use std::collections::BTreeMap;

use qsh::syntax::Parser;

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

/// Pieces random scripts are made of: tokens, quoting and expansion
/// starts, and plain characters.
const PIECES: &[&str] = &[
    " ", " ", " ", "\n", "\t", ";", ";;", "&", "&&", "|", "||", "(", ")", "{", "}", "<", ">", ">>",
    "<<", "<<-", "<&", ">&", "<>", ">|", "2>", "'", "\"", "`", "\\", "\\\n", "$", "${", "$(",
    "$((", "))", "$@", "$*", "$#", "${x:-", "${#", "${x%%", "}", "#", "~", "*", "?", "[", "]", "!",
    "=", "x=", "if", "then", "else", "elif", "fi", "while", "until", "do", "done", "for", "in",
    "case", "esac", "f()", "echo", "a", "b", "1", "EOF", "é", "\u{0}", "-", "+", ":", ",", "%",
    "^",
];

fn random_script(rng: &mut Rng, len: usize) -> String {
    let mut s = String::new();
    for _ in 0..len {
        s.push_str(PIECES[rng.below(PIECES.len())]);
    }
    s
}

fn parse_everything(src: &str, aliases: &BTreeMap<String, String>) {
    let mut p = Parser::new(src).with_aliases(aliases);
    for _ in 0..1000 {
        match p.next_command() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    let mut p = Parser::new(src).partial();
    let _ = p.parse_all();
    let _ = qsh::syntax::parse_here_text(src);
}

#[test]
fn parser_never_panics_on_random_input() {
    let mut aliases = BTreeMap::new();
    aliases.insert("a".to_string(), "b ".to_string());
    aliases.insert("b".to_string(), "a".to_string());
    aliases.insert("if".to_string(), "x".to_string());
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for i in 0..20_000 {
        let len = 1 + rng.below(40);
        let s = random_script(&mut rng, len);
        let r = std::panic::catch_unwind(|| parse_everything(&s, &aliases));
        assert!(r.is_ok(), "case {i} panicked: {s:?}");
    }
}

#[test]
fn parser_never_panics_on_random_bytes() {
    let mut rng = Rng(42);
    for _ in 0..5_000 {
        let len = rng.below(60);
        let bytes: Vec<u8> = (0..len)
            .map(|_| u8::try_from(rng.below(128)).unwrap_or(0))
            .collect();
        let s = String::from_utf8_lossy(&bytes).into_owned();
        let r = std::panic::catch_unwind(|| parse_everything(&s, &BTreeMap::new()));
        assert!(r.is_ok(), "panicked: {s:?}");
    }
}

/// Run `f` on a thread with a 2 MB stack, the default for spawned threads.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no overflow");
}

#[test]
fn deep_nesting_is_an_error_not_an_overflow() {
    on_small_stack(|| {
        for open in ["(", "{ ", "$(", "\"$(", "${x:-", "$((", "`", "if "] {
            let src = open.repeat(5000);
            let _ = Parser::new(&src).parse_all();
        }
        let arith = format!("echo $(({}1{}))", "(".repeat(3000), ")".repeat(3000));
        let o = common::run(&arith);
        assert_ne!(o.status, 0);
        let unary = format!("echo $(({}1))", "-".repeat(3000));
        let _ = common::run(&unary);
    });
}

#[test]
fn deep_recursion_in_scripts_is_an_error() {
    on_small_stack(|| {
        let o = common::run("f() { f; }; f");
        assert_eq!(o.status, 2);
        let o = common::run("e='eval \"$e\"'; eval \"$e\"");
        assert_ne!(o.status, 0);
        let o = common::run("x='$((x))'; echo $((x))");
        assert_ne!(o.status, 0);
    });
}

#[test]
fn random_scripts_run_without_panicking() {
    // Only constructs that always end: no loops, no recursion.
    const SAFE: &[&str] = &[
        "echo a",
        "x=1",
        "y=$x",
        "echo ${x:-d}",
        "echo $((1+2))",
        "false",
        "true",
        "echo $(echo s)",
        "set -- a b",
        "shift",
        "case $x in 1) echo one;; esac",
        "if true; then echo t; fi",
        "unset x",
        "echo \"$@\"",
        "readonly z=1",
        "z=2",
        "echo ${#x}",
        "echo ${x%1}",
        "export x",
        "cd /tmp",
        "cd /none",
        "echo *",
        "echo >f",
        "cat <f",
        "echo x | cat",
        "trap 'echo t' EXIT",
        "set -u",
        "echo $nosuch",
        "f() { echo f; }",
        "f",
        "local q",
        "return 1",
        "break",
        "continue",
        "exit 3",
        "echo ${q?err}",
        "eval 'echo e'",
        ". /nope",
        "alias e=echo",
        "type e",
        "command -v cat",
        "read v",
        "printf '%d\\n' 7",
        "test -n x",
        "[ 1 -lt 2 ]",
        "getopts a o -a",
        "umask 077",
        "wait",
        "true &",
        "jobs",
        "kill -0 $$",
        "hash cat",
        "let x=x+1",
        "declare -i i=5",
        "times",
        "ulimit",
        "set -e",
        "set +e",
        "! false",
        "{ echo b; }",
        "(echo s)",
    ];
    let mut rng = Rng(7);
    for _ in 0..300 {
        let n = 1 + rng.below(8);
        let parts: Vec<&str> = (0..n).map(|_| SAFE[rng.below(SAFE.len())]).collect();
        let script = parts.join("\n");
        let s = script.clone();
        let r = std::panic::catch_unwind(move || common::run(&s));
        assert!(r.is_ok(), "panicked: {script:?}");
    }
}
