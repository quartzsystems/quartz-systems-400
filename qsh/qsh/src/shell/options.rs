// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `set` options.

/// A shell option.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Opt {
    /// `-a`.
    AllExport,
    /// `-b`.
    Notify,
    /// `-C`.
    NoClobber,
    /// `-e`.
    ErrExit,
    /// `-f`.
    NoGlob,
    /// `-m`.
    Monitor,
    /// `-n`.
    NoExec,
    /// `-u`.
    NoUnset,
    /// `-v`.
    Verbose,
    /// `-x`.
    XTrace,
    /// `-o ignoreeof`.
    IgnoreEof,
    /// `-o nolog`.
    NoLog,
    /// `-t`, `-o trace`: commands are also written to the trace file.
    Trace,
    /// `-i` (invocation only).
    Interactive,
    /// `-s` (invocation only).
    Stdin,
}

/// Option name, letter (if any) and value.
pub const OPTIONS: &[(Opt, &str, Option<char>)] = &[
    (Opt::AllExport, "allexport", Some('a')),
    (Opt::ErrExit, "errexit", Some('e')),
    (Opt::IgnoreEof, "ignoreeof", None),
    (Opt::Monitor, "monitor", Some('m')),
    (Opt::NoClobber, "noclobber", Some('C')),
    (Opt::NoExec, "noexec", Some('n')),
    (Opt::NoGlob, "noglob", Some('f')),
    (Opt::NoLog, "nolog", None),
    (Opt::Notify, "notify", Some('b')),
    (Opt::NoUnset, "nounset", Some('u')),
    (Opt::Trace, "trace", Some('t')),
    (Opt::Verbose, "verbose", Some('v')),
    (Opt::XTrace, "xtrace", Some('x')),
];

/// The option set.
#[derive(Clone, Debug, Default)]
pub struct Options {
    on: Vec<Opt>,
}

impl Options {
    /// Is it on?
    pub fn get(&self, o: Opt) -> bool {
        self.on.contains(&o)
    }

    /// Turn it on or off.
    pub fn set(&mut self, o: Opt, v: bool) {
        self.on.retain(|x| *x != o);
        if v {
            self.on.push(o);
        }
    }

    /// The option for a letter, invocation-only letters included.
    pub fn by_letter(c: char) -> Option<Opt> {
        match c {
            'i' => Some(Opt::Interactive),
            's' => Some(Opt::Stdin),
            _ => OPTIONS
                .iter()
                .find(|(_, _, l)| *l == Some(c))
                .map(|(o, _, _)| *o),
        }
    }

    /// The option for a `-o` name.
    pub fn by_name(n: &str) -> Option<Opt> {
        OPTIONS
            .iter()
            .find(|(_, name, _)| *name == n)
            .map(|(o, _, _)| *o)
    }

    /// `$-`: the letters of the options that are on.
    pub fn flags(&self) -> String {
        let mut s = String::new();
        for c in [
            'a', 'b', 'C', 'e', 'f', 'i', 'm', 'n', 's', 't', 'u', 'v', 'x',
        ] {
            if Self::by_letter(c).is_some_and(|o| self.get(o)) {
                s.push(c);
            }
        }
        s
    }
}
