// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Parameter expansion.

use super::{Builder, Ctx, Q, S};
use crate::msg::Msg;
use crate::pattern::Compiled;
use crate::shell::options::Opt;
use crate::shell::{Flow, Shell};
use crate::syntax::{Param, ParamName, ParamOp, TestOp, Word};

fn display(name: &ParamName) -> String {
    match name {
        ParamName::Var(n) => n.clone(),
        ParamName::Positional(n) => n.to_string(),
        ParamName::Special(c) => c.to_string(),
    }
}

/// Remove the shortest or longest matching prefix or suffix.
fn trim(v: &str, pat: &Compiled, suffix: bool, longest: bool) -> String {
    let chars: Vec<char> = v.chars().collect();
    let n = chars.len();
    if suffix {
        let order: Vec<usize> = if longest {
            (0..=n).collect()
        } else {
            (0..=n).rev().collect()
        };
        for i in order {
            if pat.matches(&chars[i..]) {
                return chars[..i].iter().collect();
            }
        }
    } else {
        let order: Vec<usize> = if longest {
            (0..=n).rev().collect()
        } else {
            (0..=n).collect()
        };
        for i in order {
            if pat.matches(&chars[..i]) {
                return chars[i..].iter().collect();
            }
        }
    }
    v.to_string()
}

impl Shell {
    /// A parameter's value; `None` when unset.
    pub(crate) fn param_get(&mut self, name: &ParamName) -> Option<String> {
        match name {
            ParamName::Var(n) => self.var(n),
            ParamName::Positional(k) => self.args.get(k - 1).cloned(),
            ParamName::Special(c) => match c {
                '0' => Some(self.arg0.clone()),
                '?' => Some(self.status.to_string()),
                '$' => Some(self.pid.to_string()),
                '!' => self.last_bg.map(|p| p.to_string()),
                '#' => Some(self.args.len().to_string()),
                '-' => Some(self.opts.flags()),
                '@' | '*' => Some(self.star()),
                _ => None,
            },
        }
    }

    /// `"$*"`: the positional parameters joined by the first `IFS`
    /// character.
    fn star(&mut self) -> String {
        let sep = match self.var("IFS") {
            None => " ".to_string(),
            Some(s) => s.chars().next().map(String::from).unwrap_or_default(),
        };
        self.args.join(&sep)
    }

    fn unset_error(&mut self, name: &ParamName) -> Flow {
        self.report(&Msg::Unset(display(name)));
        Flow::Error(2)
    }

    /// A value for `$x` where `set -u` makes an unset one an error.
    fn value_checked(&mut self, name: &ParamName) -> Result<Option<String>, Flow> {
        let v = self.param_get(name);
        if v.is_none() && self.opts.get(Opt::NoUnset) && !matches!(name, ParamName::Special(_)) {
            return Err(self.unset_error(name));
        }
        Ok(v)
    }

    /// Push `$@` or `$*` unquoted, or `"$@"`.
    fn push_positional(&mut self, star: bool, ctx: Ctx, b: &mut Builder) {
        if ctx.dq && star {
            let s = self.star();
            b.push(&s, Q);
            return;
        }
        let args = self.args.clone();
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                b.brk();
            }
            if ctx.dq {
                b.keep();
                b.push(a, Q);
            } else {
                b.push(a, S);
            }
        }
    }

    /// Expand a word into one string (the `=` and `?` forms).
    fn word_string(&mut self, w: &Word, dq: bool) -> Result<String, Flow> {
        let mut b = Builder::new();
        let ctx = Ctx {
            dq,
            lit_split: false,
        };
        let parts = if dq {
            &w.parts[..]
        } else {
            self.tilde_prefix(&w.parts, false, ctx, &mut b)
        };
        for p in parts {
            self.part(p, ctx, &mut b)?;
        }
        Ok(b.fields
            .iter()
            .map(super::text_of)
            .collect::<Vec<_>>()
            .join(" "))
    }

    /// Expand `${...}` into the builder.
    pub(crate) fn param(&mut self, p: &Param, ctx: Ctx, b: &mut Builder) -> Result<(), Flow> {
        let flags = if ctx.dq { Q } else { S };
        let positional_all = matches!(p.name, ParamName::Special('@' | '*'));
        match &p.op {
            ParamOp::Plain => {
                if positional_all {
                    self.push_positional(p.name == ParamName::Special('*'), ctx, b);
                    return Ok(());
                }
                if let Some(v) = self.value_checked(&p.name)? {
                    b.push(&v, flags);
                }
            }
            ParamOp::Length => {
                let n = if positional_all {
                    self.args.len()
                } else {
                    self.value_checked(&p.name)?
                        .unwrap_or_default()
                        .chars()
                        .count()
                };
                b.push(&n.to_string(), flags);
            }
            ParamOp::Test { op, colon, word } => {
                let v = if positional_all {
                    if self.args.is_empty() {
                        None
                    } else {
                        Some(self.star())
                    }
                } else {
                    self.param_get(&p.name)
                };
                let missing = match &v {
                    None => true,
                    Some(s) => *colon && s.is_empty(),
                };
                let sub = Ctx {
                    dq: ctx.dq,
                    lit_split: !ctx.dq,
                };
                match op {
                    TestOp::Default | TestOp::Alternate => {
                        let use_word = if *op == TestOp::Default {
                            missing
                        } else {
                            !missing
                        };
                        if use_word {
                            let rest = if ctx.dq {
                                &word.parts[..]
                            } else {
                                self.tilde_prefix(&word.parts, false, sub, b)
                            };
                            for part in rest {
                                self.part(part, sub, b)?;
                            }
                        } else if *op == TestOp::Default {
                            if positional_all {
                                self.push_positional(p.name == ParamName::Special('*'), ctx, b);
                            } else if let Some(v) = v {
                                b.push(&v, flags);
                            }
                        }
                    }
                    TestOp::Assign => {
                        if missing {
                            let s = self.word_string(word, ctx.dq)?;
                            match &p.name {
                                ParamName::Var(n) => {
                                    let n = n.clone();
                                    self.assign(&n, &s)?;
                                }
                                other => {
                                    self.report(&Msg::BadSubst(format!(
                                        "${{{}={}}}",
                                        display(other),
                                        s
                                    )));
                                    return Err(Flow::Error(2));
                                }
                            }
                            b.push(&s, flags);
                        } else if positional_all {
                            self.push_positional(p.name == ParamName::Special('*'), ctx, b);
                        } else if let Some(v) = v {
                            b.push(&v, flags);
                        }
                    }
                    TestOp::Error => {
                        if missing {
                            let s = self.word_string(word, ctx.dq)?;
                            let text = if s.is_empty() {
                                if v.is_none() {
                                    "the parameter is not set".to_string()
                                } else {
                                    "the parameter is null or not set".to_string()
                                }
                            } else {
                                s
                            };
                            self.report(&Msg::ParamError(display(&p.name), text));
                            return Err(Flow::Error(2));
                        }
                        if positional_all {
                            self.push_positional(p.name == ParamName::Special('*'), ctx, b);
                        } else if let Some(v) = v {
                            b.push(&v, flags);
                        }
                    }
                }
            }
            ParamOp::Trim {
                suffix,
                longest,
                pattern,
            } => {
                let v = if positional_all {
                    Some(self.star())
                } else {
                    self.value_checked(&p.name)?
                };
                let pat = self.expand_pattern(pattern)?;
                let r = trim(
                    &v.unwrap_or_default(),
                    &Compiled::new(&pat),
                    *suffix,
                    *longest,
                );
                b.push(&r, flags);
            }
        }
        Ok(())
    }
}
