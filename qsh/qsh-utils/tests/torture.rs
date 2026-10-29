// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sort` and `sed` torture: random inputs through the utilities on the
//! seeded `MemSys`, checked against small reference implementations, and
//! large or pathological inputs that must finish.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::items_after_statements,
    clippy::format_collect
)]

mod common;

use std::cmp::Ordering;

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn word(&mut self) -> String {
        let len = 1 + self.below(5);
        (0..len)
            .map(|_| {
                let c = b"abcABC_xyz"[self.below(10)];
                c as char
            })
            .collect()
    }
}

/// Run a script with a file `in.txt` holding `input`.
fn run_with(script: &str, input: &str) -> common::Outcome {
    let s = common::sys();
    s.write_file("/home/QSHUSER/in.txt", input);
    common::run_on(&s, script, "")
}

fn lines(n: usize, rng: &mut Rng) -> Vec<String> {
    (0..n)
        .map(|_| {
            let num = rng.below(200) as i64 - 100;
            format!("{} {} {}", rng.word(), num, rng.word())
        })
        .collect()
}

fn joined(v: &[String]) -> String {
    let mut s = v.join("\n");
    s.push('\n');
    s
}

fn field(l: &str, n: usize) -> &str {
    l.split(' ').nth(n).unwrap_or("")
}

#[test]
fn sort_matches_reference() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for round in 0..12 {
        let v = lines(200 + round * 10, &mut rng);
        let input = joined(&v);
        type Cmp = fn(&String, &String) -> Ordering;
        let cases: &[(&str, Cmp, bool)] = &[
            ("sort in.txt", |a, b| a.as_bytes().cmp(b.as_bytes()), false),
            ("sort -r in.txt", |a, b| b.as_bytes().cmp(a.as_bytes()), false),
            (
                "sort -f in.txt",
                |a, b| {
                    a.to_ascii_uppercase()
                        .cmp(&b.to_ascii_uppercase())
                        .then_with(|| a.as_bytes().cmp(b.as_bytes()))
                },
                false,
            ),
            (
                "sort -t ' ' -k2,2n in.txt",
                |a, b| {
                    let x: i64 = field(a, 1).parse().unwrap_or(0);
                    let y: i64 = field(b, 1).parse().unwrap_or(0);
                    x.cmp(&y).then_with(|| a.as_bytes().cmp(b.as_bytes()))
                },
                false,
            ),
            (
                "sort -t ' ' -k2,2nr -k1,1 in.txt",
                |a, b| {
                    let x: i64 = field(a, 1).parse().unwrap_or(0);
                    let y: i64 = field(b, 1).parse().unwrap_or(0);
                    y.cmp(&x)
                        .then_with(|| field(a, 0).as_bytes().cmp(field(b, 0).as_bytes()))
                        .then_with(|| a.as_bytes().cmp(b.as_bytes()))
                },
                false,
            ),
            (
                "sort -t ' ' -k3,3 -u in.txt",
                |a, b| field(a, 2).as_bytes().cmp(field(b, 2).as_bytes()),
                true,
            ),
            (
                "sort -u in.txt",
                |a, b| a.as_bytes().cmp(b.as_bytes()),
                true,
            ),
        ];
        for (script, cmp, unique) in cases {
            let mut want = v.clone();
            want.sort_by(cmp);
            if *unique {
                want.dedup_by(|a, b| cmp(a, b) == Ordering::Equal);
            }
            let o = run_with(script, &input);
            assert_eq!(o.status, 0, "{script}: {}", o.err);
            assert_eq!(o.out, joined(&want), "{script} round {round}");
        }
        // -c agrees with the order it produced.
        let o = run_with("sort in.txt > s.txt; sort -c s.txt; echo $?", &input);
        assert_eq!(o.out, "0\n");
    }
}

#[test]
fn sort_numbers_against_reference() {
    let mut rng = Rng(7);
    let nums: Vec<String> = (0..500)
        .map(|_| {
            let whole = rng.below(100_000) as i64 - 50_000;
            let frac = rng.below(1000);
            match rng.below(3) {
                0 => whole.to_string(),
                1 => format!("{whole}.{frac:03}"),
                _ => format!("  {whole}"),
            }
        })
        .collect();
    let mut want = nums.clone();
    want.sort_by(|a, b| {
        let x: f64 = a.trim().parse().unwrap_or(0.0);
        let y: f64 = b.trim().parse().unwrap_or(0.0);
        x.partial_cmp(&y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.as_bytes().cmp(b.as_bytes()))
    });
    let o = run_with("sort -n in.txt", &joined(&nums));
    assert_eq!(o.out, joined(&want));
}

#[test]
fn sed_matches_reference() {
    let mut rng = Rng(99);
    let v = lines(400, &mut rng);
    let input = joined(&v);
    let check = |script: &str, want: String| {
        let o = run_with(script, &input);
        assert_eq!(o.status, 0, "{script}: {}", o.err);
        assert_eq!(o.out, want, "{script}");
    };
    check(
        "sed 's/a/#/g' in.txt",
        joined(&v.iter().map(|l| l.replace('a', "#")).collect::<Vec<_>>()),
    );
    check(
        "sed 's/a/#/' in.txt",
        joined(&v.iter().map(|l| l.replacen('a', "#", 1)).collect::<Vec<_>>()),
    );
    check(
        "sed 's/\\([^ ]*\\) \\([^ ]*\\)/\\2 \\1/' in.txt",
        joined(
            &v.iter()
                .map(|l| {
                    let f: Vec<&str> = l.split(' ').collect();
                    format!("{} {} {}", f[1], f[0], f[2])
                })
                .collect::<Vec<_>>(),
        ),
    );
    check(
        "sed 'y/abc/ABC/' in.txt",
        joined(
            &v.iter()
                .map(|l| l.replace('a', "A").replace('b', "B").replace('c', "C"))
                .collect::<Vec<_>>(),
        ),
    );
    check("sed '$!N;P;D' in.txt", input.clone());
    check("sed -n '1!G;h;$p' in.txt", {
        let mut r = v.clone();
        r.reverse();
        joined(&r)
    });
    check("sed ':a;N;$!ba;s/\\n/,/g' in.txt", format!("{}\n", v.join(",")));
    check("sed -n '$=' in.txt", format!("{}\n", v.len()));
    check(
        "sed -n '/^a/p' in.txt",
        joined(&v.iter().filter(|l| l.starts_with('a')).cloned().collect::<Vec<_>>()),
    );
    check(
        "sed '/x/d' in.txt",
        joined(&v.iter().filter(|l| !l.contains('x')).cloned().collect::<Vec<_>>()),
    );
    check(
        "sed -n '10,20p' in.txt",
        joined(&v[9..20]),
    );
    check(
        "sed 'n;d' in.txt",
        joined(&v.iter().step_by(2).cloned().collect::<Vec<_>>()),
    );
    check(
        "sed = in.txt | sed 'N;s/\\n/ /'",
        joined(
            &v.iter()
                .enumerate()
                .map(|(i, l)| format!("{} {l}", i + 1))
                .collect::<Vec<_>>(),
        ),
    );
}

#[test]
fn sed_large_and_pathological_inputs_finish() {
    let long = "x".repeat(100_000);
    let o = run_with("sed 's/x*/-/g' in.txt | wc -c", &format!("{long}\n"));
    assert_eq!(o.out.trim(), "2");
    let o = run_with("sed 's/x/y/g' in.txt | grep -c y", &format!("{long}\n"));
    assert_eq!(o.out.trim(), "1");
    let many: String = (0..20_000).map(|i| format!("{i}\n")).collect();
    let o = run_with("sed -n '$p' in.txt", &many);
    assert_eq!(o.out, "19999\n");
    // A loop that ends when nothing is left to change.
    let o = run_with(":", "");
    assert_eq!(o.status, 0);
    let o = run_with(
        "sed -e ':a' -e 's/aa/a/' -e 'ta' in.txt",
        &format!("{}\n", "a".repeat(2000)),
    );
    assert_eq!(o.out, "a\n");
    let o = run_with(
        "sed -E 's/(a|aa)*b/X/' in.txt",
        &format!("{}\n", "a".repeat(3000)),
    );
    assert_eq!(o.out.len(), 3001);
}
