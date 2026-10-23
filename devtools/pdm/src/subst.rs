// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Substitution in option commands: `&L` library, `&F` file, `&N`
//! name, `&O` object library, `&T` type, and the others PDM documents
//! (`&A` attribute, `&B` list type, `&C` option, `&E` run in batch,
//! `&H`/`&J` job description and library, `&R` replace, `&X` text,
//! `&U` user). `&&` is one ampersand; any other `&` stays as typed.

use crate::defaults::{Defaults, yes_no};
use crate::option::ListKind;

/// An entry of a PDM list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Item {
    /// The library: the item itself on a library list, the object's
    /// on an object list, the file's on a member list.
    pub library: String,
    /// The source file of a member; blank elsewhere.
    pub file: String,
    /// The item's name.
    pub name: String,
    /// The object type (`*PGM`), member type (`CLLE`) or library type
    /// (`PROD`).
    pub ty: String,
    /// The object attribute (`CLLE`, `PF`); blank for members.
    pub attr: String,
    /// The text.
    pub text: String,
}

/// What a substitution knows besides the item.
pub struct Context<'a> {
    /// The list the item is on.
    pub list: ListKind,
    /// The option typed.
    pub option: &'a str,
    /// The session defaults.
    pub defaults: &'a Defaults,
    /// The resolved object library.
    pub object_lib: &'a str,
    /// The user running PDM.
    pub user: &'a str,
}

/// The value of one substitution letter, `None` when the letter is not
/// one.
fn value(c: char, item: &Item, ctx: &Context<'_>) -> Option<String> {
    Some(match c.to_ascii_uppercase() {
        'A' => item.attr.clone(),
        'B' => ctx.list.letter().to_string(),
        'C' => ctx.option.to_string(),
        'E' => yes_no(ctx.defaults.batch_run).to_string(),
        'F' => item.file.clone(),
        'H' => ctx.defaults.jobd.clone(),
        'J' => ctx.defaults.jobd_lib.clone(),
        'L' => item.library.clone(),
        'N' => item.name.clone(),
        'O' => ctx.object_lib.to_string(),
        'R' => yes_no(ctx.defaults.replace).to_string(),
        'T' => item.ty.clone(),
        'U' => ctx.user.to_string(),
        'X' => format!("'{}'", item.text.trim_end().replace('\'', "''")),
        _ => return None,
    })
}

/// `template` with the item's values in place of the `&` letters.
#[must_use]
pub fn substitute(template: &str, item: &Item, ctx: &Context<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut it = template.chars().peekable();
    while let Some(c) = it.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some('&') => {
                it.next();
                out.push('&');
            }
            Some(l) => match value(l, item, ctx) {
                Some(v) => {
                    it.next();
                    out.push_str(v.trim_end());
                }
                None => out.push('&'),
            },
            None => out.push('&'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters() {
        let d = Defaults::default();
        let item = Item {
            library: "MYLIB".into(),
            file: "QCLSRC".into(),
            name: "HELLO".into(),
            ty: "CLLE".into(),
            attr: String::new(),
            text: "Say it's hello".into(),
        };
        let ctx = Context {
            list: ListKind::Member,
            option: "14",
            defaults: &d,
            object_lib: "OBJS",
            user: "QPGMR",
        };
        assert_eq!(
            substitute("CRTBNDCL PGM(&O/&N) SRCFILE(&L/&F) SRCMBR(&n)", &item, &ctx),
            "CRTBNDCL PGM(OBJS/HELLO) SRCFILE(MYLIB/QCLSRC) SRCMBR(HELLO)"
        );
        assert_eq!(
            substitute("&T &B &C &R &E", &item, &ctx),
            "CLLE M 14 *NO *NO"
        );
        assert_eq!(
            substitute("TEXT(&X)", &item, &ctx),
            "TEXT('Say it''s hello')"
        );
        assert_eq!(substitute("A && B &Q &", &item, &ctx), "A & B &Q &");
        assert_eq!(substitute("&H/&J &U", &item, &ctx), "*USRPRF/ QPGMR");
    }
}
