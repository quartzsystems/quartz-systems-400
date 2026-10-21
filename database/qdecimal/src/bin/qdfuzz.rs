// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qdfuzz`: generate the oracle kit, our expected rows, and diff the
//! machine's results. See `conformance/oracle/qdecimal/README.md`.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use qdecimal::fuzz;

const USAGE: &str = "\
qdfuzz kit <dir> [--random N] [--seed S]   write cases.csv, expected.csv, QDFUZZ.RPGLE,
                                           QDFUZZIN.PF, QDFUZZOUT.PF and README.md
qdfuzz expected <cases.csv> <out.csv>      our rows for a case file
qdfuzz diff <cases.csv> <results.csv>      compare the machine's rows with ours
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("qdfuzz: {e}");
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let strs: Vec<&str> = args.iter().map(String::as_str).collect();
    match strs.as_slice() {
        ["kit", dir, rest @ ..] => {
            let (random, seed) = options(rest)?;
            kit(Path::new(dir), random, seed)?;
            Ok(ExitCode::SUCCESS)
        }
        ["expected", cases, out] => {
            let cases = fuzz::read_cases(&read(cases)?)?;
            let rows: Vec<_> = cases.iter().map(fuzz::expected).collect();
            write(Path::new(out), &fuzz::write_rows(&rows))?;
            println!("{} row(s) written to {out}", rows.len());
            Ok(ExitCode::SUCCESS)
        }
        ["diff", cases, results] => {
            let cases = fuzz::read_cases(&read(cases)?)?;
            let rows = fuzz::read_rows(&read(results)?)?;
            let report = fuzz::diff(&cases, &rows);
            println!("{report}");
            Ok(if report.mismatches.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        _ => Err("bad arguments".to_string()),
    }
}

fn options(rest: &[&str]) -> Result<(usize, u64), String> {
    let mut random = 1000;
    let mut seed = 20_261_010;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match *flag {
            "--random" => random = value.parse().map_err(|_| "bad --random".to_string())?,
            "--seed" => seed = value.parse().map_err(|_| "bad --seed".to_string())?,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok((random, seed))
}

fn kit(dir: &Path, random: usize, seed: u64) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let cases = fuzz::generate(seed, random);
    let rows: Vec<_> = cases.iter().map(fuzz::expected).collect();
    write(&dir.join("cases.csv"), &fuzz::write_cases(&cases))?;
    write(&dir.join("expected.csv"), &fuzz::write_rows(&rows))?;
    write(&dir.join("QDFUZZ.RPGLE"), &fuzz::rpg::rpg_source())?;
    write(&dir.join("QDFUZZIN.PF"), &fuzz::rpg::dds_in())?;
    write(&dir.join("QDFUZZOUT.PF"), &fuzz::rpg::dds_out())?;
    write(&dir.join("README.md"), &fuzz::rpg::readme())?;
    println!("{} case(s) written to {}", cases.len(), dir.display());
    Ok(())
}

fn read(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
