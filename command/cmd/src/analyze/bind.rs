// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Binding: positional and keyword values to parameters, lists and
//! groups to their elements, special and single values to their
//! replacements, defaults where nothing was given.

use super::{Bound, Env, conv, default_tokens, expr, is_elem, is_qual, positional_ok};
use crate::def::{CmdDef, Parm, ParmType, Special};
use crate::error::Diag;
use crate::token::{Sel, Tok};
use crate::value::Value;

/// What the string gave a parameter: the tokens, the selective-prompt
/// prefix and the position of its keyword in the string.
type Given = (Vec<Tok>, Option<Sel>, usize);

/// The order the box reports in: too many positional values; a level
/// of parentheses too many and a job number that is not one; a keyword
/// it does not know; a required parameter missing; then the values in
/// order.
fn rank(msgid: &str) -> u8 {
    match msgid {
        "CPD0065" => 0,
        "CPD0012" | "CPD0047" | "CPD0048" => 1,
        "CPD0011" | "CPD0042" | "CPD0043" => 2,
        "CPD0071" => 3,
        _ => 4,
    }
}

/// The binding of one command.
pub(super) struct Ctx<'e, 'a> {
    /// The environment.
    pub env: &'e mut Env<'a>,
    /// The definition.
    pub def: &'e CmdDef,
    /// What went wrong.
    pub diags: Vec<Diag>,
    /// The command is about to be prompted: a required parameter may
    /// still be missing, the display will ask for it.
    pub prompting: bool,
}

impl Ctx<'_, '_> {
    fn bad(&mut self, id: &str, kwd: &str, detail: String) {
        self.diags
            .push(Diag::new(id).data(&[(kwd, 10)]).parm(kwd).detail(detail));
    }

    /// Bind every parameter of the definition from the command's
    /// tokens. The diagnostics come out as the box reports them: a
    /// missing required parameter first, the rest in the order of the
    /// string.
    pub fn bind_all(&mut self, toks: &[Tok]) -> Vec<Bound> {
        let n = self.def.parms.len();
        let mut given: Vec<Option<Given>> = vec![None; n];
        let mut ordered: Vec<(usize, Diag)> = Vec::new();
        let mut pos = 0;
        let mut seen_kw = false;
        for (at, t) in toks.iter().enumerate() {
            let before = self.diags.len();
            match t {
                Tok::Kw { name, pfx, args } => {
                    seen_kw = true;
                    if let Some(i) = self.def.index(name) {
                        if given[i].is_some() {
                            self.bad("CPD0042", name, format!("{name} was given twice"));
                        } else {
                            given[i] = Some((args.clone(), *pfx, at + 1));
                        }
                    } else {
                        self.bad(
                            "CPD0043",
                            name,
                            format!("{name} is not a keyword of {}", self.def.name),
                        );
                    }
                }
                Tok::Word(w) if w.starts_with("?-") || w.starts_with("?&") => {
                    let (sel, name) = match w.strip_prefix("?-") {
                        Some(rest) => (Sel::Omit, rest),
                        None => (Sel::Hidden, &w[2..]),
                    };
                    match self.def.index(name) {
                        Some(i) => given[i] = Some((Vec::new(), Some(sel), at + 1)),
                        None => self.bad(
                            "CPD0043",
                            name,
                            format!("{name} is not a keyword of {}", self.def.name),
                        ),
                    }
                }
                other => {
                    if seen_kw {
                        self.bad(
                            "CPD0011",
                            &other.text(),
                            format!("{} is positional after a keyword", other.text()),
                        );
                    } else if !positional_ok(self.def, pos) {
                        self.bad(
                            "CPD0065",
                            &other.text(),
                            format!("{} is beyond the positional parameters", other.text()),
                        );
                        pos += 1;
                    } else {
                        if other.word() != Some("*N") {
                            // A positional group comes in its own parentheses.
                            let args = match (other, self.def.parms.get(pos)) {
                                (Tok::Paren(items), Some(p)) if is_elem(p) && !p.is_list() => {
                                    items.clone()
                                }
                                _ => vec![other.clone()],
                            };
                            given[pos] = Some((args, None, at + 1));
                        }
                        pos += 1;
                    }
                }
            }
            ordered.extend(self.diags.drain(before..).map(|d| (at + 1, d)));
        }
        let mut out = Vec::with_capacity(n);
        for (i, parm) in self.def.parms.iter().enumerate() {
            let (args, sel, at) = given[i].take().unwrap_or((Vec::new(), None, 0));
            let specified = !args.is_empty();
            let before = self.diags.len();
            let value = if specified {
                self.bind_parm(parm, &parm.kwd, &args, false)
            } else {
                if parm.required() && parm.constant.is_none() && !self.prompting {
                    self.bad("CPD0071", &parm.kwd, format!("{} is required", parm.kwd));
                }
                self.default_of(parm, &parm.kwd)
            };
            ordered.extend(self.diags.drain(before..).map(|d| (at, d)));
            out.push(Bound {
                kwd: parm.kwd.clone(),
                value,
                specified,
                sel,
                given: args,
            });
        }
        ordered.sort_by_key(|(p, d)| (rank(&d.msgid), *p));
        self.diags.extend(ordered.into_iter().map(|(_, d)| d));
        out
    }

    /// A parameter's `CONSTANT` or `DFT` value; a group without one
    /// takes its elements' defaults.
    pub fn default_of(&mut self, parm: &Parm, kwd: &str) -> Option<Value> {
        let Some(text) = parm.constant.as_ref().or(parm.dft.as_ref()) else {
            if is_elem(parm)
                && !parm.is_list()
                && parm.items.iter().any(|i| i.dft.is_some())
                && !parm.items.iter().any(Parm::required)
            {
                return self.bind_group(parm, kwd, &[], true);
            }
            return None;
        };
        let toks = default_tokens(text, self.env.opts);
        if toks.is_empty() {
            return None;
        }
        self.bind_parm(parm, kwd, &toks, true)
    }

    /// Bind the tokens of one parameter (or of one element, or one
    /// qualifier): a list, a group, a qualified name or a scalar.
    pub fn bind_parm(
        &mut self,
        parm: &Parm,
        kwd: &str,
        toks: &[Tok],
        trusted: bool,
    ) -> Option<Value> {
        if let [one] = toks
            && let Some(v) = self.single(parm, kwd, one, &parm.sngval)
        {
            return v;
        }
        if parm.is_list() {
            return self.bind_list(parm, kwd, toks, trusted);
        }
        if is_elem(parm) {
            // A single group's elements come bare: `KWD((a b))` is a
            // level too many (the box tells a group whose first element
            // is a qualified name apart from the rest).
            let inner = match toks {
                [Tok::Paren(items)] if !trusted => {
                    let qual_first = parm.items.first().is_some_and(is_qual);
                    self.bad(
                        if qual_first { "CPD0048" } else { "CPD0047" },
                        kwd,
                        format!("{kwd}: too many levels of parentheses"),
                    );
                    return None;
                }
                [Tok::Paren(items)] => items.as_slice(),
                other => other,
            };
            return self.bind_group(parm, kwd, inner, trusted);
        }
        if is_qual(parm) {
            return self.bind_qual(parm, kwd, toks, trusted);
        }
        match toks {
            [] => None,
            [one] => self.bind_scalar(parm, kwd, one, trusted),
            many => self.bind_expr(parm, kwd, many, trusted),
        }
    }

    /// A `SNGVAL` or `SPCVAL` match: the replacement value, or `None`
    /// when the token is none of them.
    #[allow(clippy::option_option)] // matched, and matched with no value, differ
    fn single(
        &mut self,
        parm: &Parm,
        kwd: &str,
        tok: &Tok,
        specials: &[Special],
    ) -> Option<Option<Value>> {
        let text = tok.text();
        let s = specials.iter().find(|s| s.from == text)?;
        Some(match &s.to {
            None => Some(Value::Char(text)),
            Some(to) => {
                let toks = default_tokens(to, self.env.opts);
                // A group's replacement takes its first element's type.
                let target = if is_elem(parm) || is_qual(parm) {
                    parm.items.first().unwrap_or(parm)
                } else {
                    parm
                };
                match toks.as_slice() {
                    [one] => self.scalar_of(target, kwd, one, true),
                    _ => Some(Value::Char(to.clone())),
                }
            }
        })
    }

    /// A scalar: special values first.
    fn bind_scalar(&mut self, parm: &Parm, kwd: &str, tok: &Tok, trusted: bool) -> Option<Value> {
        if let Some(v) = self.single(parm, kwd, tok, &parm.spcval) {
            return v;
        }
        if let Tok::Paren(items) = tok {
            // One value in parentheses, or an expression.
            return match items.as_slice() {
                [one] => self.bind_scalar(parm, kwd, one, trusted),
                many => self.bind_expr(parm, kwd, many, trusted),
            };
        }
        self.scalar_of(parm, kwd, tok, trusted)
    }

    /// Convert one token for a scalar type.
    fn scalar_of(&mut self, parm: &Parm, kwd: &str, tok: &Tok, trusted: bool) -> Option<Value> {
        if parm.ty == crate::def::ParmType::CmdStr {
            return self.cmdstr(parm, kwd, std::slice::from_ref(tok));
        }
        match conv::scalar(self.env, parm, kwd, tok, trusted) {
            Ok(v) => Some(v),
            Err(d) => {
                self.diags.push(d);
                None
            }
        }
    }

    /// Several tokens where one scalar goes: an expression when
    /// `EXPR(*YES)`, a command string, else too many values.
    fn bind_expr(&mut self, parm: &Parm, kwd: &str, toks: &[Tok], trusted: bool) -> Option<Value> {
        if parm.ty == crate::def::ParmType::CmdStr {
            return self.cmdstr(parm, kwd, toks);
        }
        // Character, numeric and logical scalars take an expression
        // whether or not the definition says `EXPR(*YES)`: IBM's own
        // definitions say it nearly everywhere, ours leave it out
        // (see `docs/CMD_OPEN_QUESTIONS.md`).
        let takes_expr = parm.expr
            || !matches!(
                parm.ty,
                ParmType::Elem { .. }
                    | ParmType::Qual { .. }
                    | ParmType::Null
                    | ParmType::ZeroElem
                    | ParmType::Cmd
                    | ParmType::CmdStr
                    | ParmType::X
            );
        if !takes_expr {
            self.bad(
                "CPD0016",
                kwd,
                format!("{kwd} takes one value; {} were given", toks.len()),
            );
            return None;
        }
        // A date or time never takes an expression.
        if matches!(parm.ty, ParmType::Date | ParmType::Time) {
            self.bad(
                "CPD0104",
                kwd,
                format!("{kwd}: an expression is not allowed for this parameter"),
            );
            return None;
        }
        match expr::eval(toks, self.env) {
            Ok(tok) => self.scalar_of(parm, kwd, &tok, trusted),
            Err(expr::Unevaluated::Variables) if self.env.vars.is_some() => {
                Some(Value::Expr(toks.to_vec()))
            }
            Err(expr::Unevaluated::Variables) => {
                self.bad(
                    "CPD0104",
                    kwd,
                    format!("{kwd}: an expression with variables is for CL programs"),
                );
                None
            }
            Err(expr::Unevaluated::Bad(why)) => {
                self.bad("CPD0047", kwd, format!("{kwd}: too many values; {why}"));
                None
            }
        }
    }

    /// A `*CMDSTR` value: the text, syntax-checked as a command.
    fn cmdstr(&mut self, parm: &Parm, kwd: &str, toks: &[Tok]) -> Option<Value> {
        let text = match toks {
            [Tok::Str(s)] => s.clone(),
            [Tok::Var(v)] if self.env.vars.is_some() => return Some(Value::Var(v.clone())),
            [Tok::Paren(items)] => Tok::join(items),
            other => Tok::join(other),
        };
        let max = parm.length().len as usize;
        if text.chars().count() > max {
            self.bad(
                "CPD0074",
                kwd,
                format!("{kwd}: the command string is longer than {max}"),
            );
            return None;
        }
        if self.env.nest >= 5 {
            self.bad(
                "CPD0084",
                kwd,
                format!("{kwd}: command strings nest too deeply"),
            );
            return None;
        }
        let saved = (self.env.setting, self.env.nest);
        self.env.nest += 1;
        // The nested command runs where the outer one says (a submitted
        // job is batch); only its syntax is checked here.
        let r = super::analyze(self.env, &text);
        self.env.setting = saved.0;
        self.env.nest = saved.1;
        match r {
            Ok(_) => Some(Value::Char(text)),
            Err(e) => {
                for d in e.diags {
                    self.diags.push(d);
                }
                self.bad(
                    "CPD0084",
                    kwd,
                    format!("{kwd}: the command string has errors"),
                );
                None
            }
        }
    }

    /// A list: each token an element; `MIN` and `MAX` counted.
    fn bind_list(&mut self, parm: &Parm, kwd: &str, toks: &[Tok], trusted: bool) -> Option<Value> {
        let group = is_elem(parm) || is_qual(parm);
        let toks: &[Tok] = match toks {
            // `KWD((a b))` for a list of scalars is the list in extra
            // parentheses; for a list of groups it is one group.
            [Tok::Paren(items)] if !group && items.iter().all(|t| !matches!(t, Tok::Paren(_))) => {
                items
            }
            other => other,
        };
        let n = toks.len();
        if n > usize::from(parm.max) {
            self.bad(
                "CPD0016",
                kwd,
                format!("{kwd}: {n} values where {} are allowed", parm.max),
            );
            return None;
        }
        if n < usize::from(parm.min) {
            self.bad(
                "CPD0071",
                kwd,
                format!("{kwd}: at least {} values are needed", parm.min),
            );
            return None;
        }
        if n > 1
            && toks
                .iter()
                .any(|t| parm.sngval.iter().any(|s| s.from == t.text()))
        {
            self.bad(
                "CPD0094",
                kwd,
                format!("{kwd}: a single value was given with other values"),
            );
            return None;
        }
        let mut items = Vec::with_capacity(n);
        let mut ok = true;
        for t in toks {
            let v = if is_elem(parm) {
                match t {
                    Tok::Paren(inner) => self.bind_group(parm, kwd, inner, trusted),
                    one => self.bind_group(parm, kwd, std::slice::from_ref(one), trusted),
                }
            } else if is_qual(parm) {
                self.bind_qual(parm, kwd, std::slice::from_ref(t), trusted)
            } else {
                self.bind_scalar(parm, kwd, t, trusted)
            };
            match v {
                Some(v) => items.push(v),
                None => ok = false,
            }
        }
        ok.then_some(Value::List(items))
    }

    /// A group: the tokens are the elements in definition order.
    fn bind_group(&mut self, parm: &Parm, kwd: &str, toks: &[Tok], trusted: bool) -> Option<Value> {
        if toks.len() > parm.items.len() {
            self.bad(
                "CPD0016",
                kwd,
                format!(
                    "{kwd}: {} elements where {} are defined",
                    toks.len(),
                    parm.items.len()
                ),
            );
            return None;
        }
        let mut out = Vec::with_capacity(parm.items.len());
        let mut ok = true;
        for (i, item) in parm.items.iter().enumerate() {
            let name = kwd.to_string();
            let v = match toks.get(i) {
                None => self.default_of(item, &name),
                Some(Tok::Word(w)) if w == "*N" => self.default_of(item, &name),
                Some(t) => {
                    let inner: &[Tok] = match t {
                        Tok::Paren(items) if item.is_list() || is_elem(item) => items,
                        other => std::slice::from_ref(other),
                    };
                    match self.bind_parm(item, &name, inner, trusted) {
                        Some(v) => Some(v),
                        None => {
                            ok = false;
                            None
                        }
                    }
                }
            };
            if item.required() && v.is_none() && ok {
                self.bad(
                    "CPD0072",
                    kwd,
                    format!("{kwd}: element {} is required", i + 1),
                );
                ok = false;
            }
            out.push(v);
        }
        ok.then_some(Value::Elems(out))
    }

    /// A qualified name: `LIB/NAME`, `(NAME LIB)` or `NAME`; parts in
    /// definition order, the object first.
    fn bind_qual(&mut self, parm: &Parm, kwd: &str, toks: &[Tok], trusted: bool) -> Option<Value> {
        let parts: Vec<Tok> = match toks {
            [Tok::Word(w)] if w.contains('/') => {
                let mut v: Vec<Tok> = w.split('/').map(|p| Tok::Word(p.to_string())).collect();
                v.reverse();
                v
            }
            [Tok::Paren(items)] => items.clone(),
            other => other.to_vec(),
        };
        if parts.len() > parm.items.len() {
            self.bad(
                "CPD0045",
                kwd,
                format!(
                    "{kwd}: a name with {} parts where {} are defined",
                    parts.len(),
                    parm.items.len()
                ),
            );
            return None;
        }
        // A single value of the parameter (`*PRINT`, `*LDA`) names the
        // whole thing: it takes no library.
        if parts.len() > 1
            && let Some(Tok::Word(w)) = parts.first()
            && w.starts_with('*')
            && parm.sngval.iter().any(|s| s.from == *w)
        {
            self.bad("CPD0078", kwd, format!("{kwd}: {w} cannot be qualified"));
            return None;
        }
        let mut out = Vec::with_capacity(parm.items.len());
        let mut ok = true;
        for (i, q) in parm.items.iter().enumerate() {
            let v = match parts.get(i) {
                None | Some(Tok::Word(_))
                    if parts.get(i).is_none_or(|t| t.word() == Some("*N")) =>
                {
                    self.default_of(q, kwd)
                }
                Some(t) => match self.bind_scalar(q, kwd, t, trusted) {
                    Some(v) => Some(v),
                    None => {
                        ok = false;
                        None
                    }
                },
                None => None,
            };
            match v {
                Some(v) => out.push(v),
                None if ok => {
                    if i == 0 {
                        self.bad("CPD0071", kwd, format!("{kwd}: a name is required"));
                        ok = false;
                    } else {
                        out.push(Value::Char(String::new()));
                    }
                }
                None => {}
            }
        }
        ok.then_some(Value::Qual(out))
    }
}
