// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! How many lines IBM's listing gives each message's text, in the
//! source and additional sections and in the message summary. Our
//! texts are our own; they are wrapped to the same number of lines so
//! that the pages break where IBM's do.

/// Identifier, lines in the source and additional sections, lines in
/// the message summary (0: not seen there).
const LINES: &[(&str, u8, u8)] = &[
    ("RNF0231", 2, 2),
    ("RNF0256", 2, 2),
    ("RNF0257", 2, 2),
    ("RNF0263", 1, 1),
    ("RNF0305", 1, 1),
    ("RNF0306", 1, 1),
    ("RNF0312", 1, 1),
    ("RNF0316", 2, 2),
    ("RNF0317", 2, 1),
    ("RNF0331", 1, 1),
    ("RNF0334", 1, 1),
    ("RNF0362", 2, 2),
    ("RNF0503", 2, 2),
    ("RNF0504", 2, 2),
    ("RNF0512", 2, 2),
    ("RNF0533", 1, 1),
    ("RNF0599", 1, 1),
    ("RNF0604", 1, 1),
    ("RNF0607", 1, 1),
    ("RNF0612", 1, 1),
    ("RNF0637", 2, 2),
    ("RNF1301", 1, 1),
    ("RNF1302", 1, 1),
    ("RNF1324", 1, 1),
    ("RNF1502", 2, 2),
    ("RNF1509", 2, 2),
    ("RNF2026", 1, 1),
    ("RNF2120", 2, 2),
    ("RNF2318", 2, 2),
    ("RNF2372", 2, 2),
    ("RNF3308", 1, 1),
    ("RNF3312", 2, 2),
    ("RNF3316", 2, 2),
    ("RNF3327", 2, 2),
    ("RNF3358", 1, 1),
    ("RNF3401", 1, 1),
    ("RNF3430", 2, 2),
    ("RNF3431", 2, 1),
    ("RNF3438", 2, 2),
    ("RNF3452", 1, 1),
    ("RNF3467", 2, 2),
    ("RNF3530", 1, 1),
    ("RNF3545", 2, 2),
    ("RNF3551", 1, 1),
    ("RNF3556", 2, 2),
    ("RNF3557", 2, 2),
    ("RNF3571", 2, 2),
    ("RNF3576", 2, 2),
    ("RNF3596", 2, 2),
    ("RNF3599", 2, 2),
    ("RNF3764", 2, 2),
    ("RNF3768", 2, 2),
    ("RNF3901", 2, 2),
    ("RNF3905", 2, 2),
    ("RNF5005", 2, 2),
    ("RNF5016", 1, 1),
    ("RNF5054", 2, 2),
    ("RNF5141", 1, 1),
    ("RNF5177", 2, 2),
    ("RNF5178", 2, 2),
    ("RNF5232", 2, 2),
    ("RNF5249", 1, 1),
    ("RNF5276", 2, 2),
    ("RNF5295", 1, 1),
    ("RNF5377", 1, 1),
    ("RNF5412", 2, 2),
    ("RNF5413", 2, 2),
    ("RNF5415", 2, 2),
    ("RNF5501", 2, 1),
    ("RNF5508", 1, 1),
    ("RNF6011", 2, 2),
    ("RNF7018", 1, 1),
    ("RNF7023", 1, 1),
    ("RNF7030", 1, 1),
    ("RNF7031", 0, 1),
    ("RNF7057", 2, 2),
    ("RNF7066", 1, 1),
    ("RNF7078", 2, 2),
    ("RNF7086", 2, 2),
    ("RNF7301", 2, 2),
    ("RNF7341", 0, 2),
    ("RNF7342", 0, 2),
    ("RNF7350", 2, 2),
    ("RNF7416", 2, 2),
    ("RNF7421", 1, 1),
    ("RNF7423", 2, 2),
    ("RNF7503", 1, 1),
    ("RNF7504", 1, 1),
    ("RNF7510", 2, 1),
    ("RNF7519", 2, 1),
    ("RNF7534", 2, 1),
    ("RNF7551", 2, 2),
    ("RNF7591", 1, 1),
];

/// The lines of a message's text where it appears with a statement
/// (`summary` false) or in the message summary.
pub(crate) fn lines(id: &str, summary: bool) -> Option<usize> {
    let (_, a, b) = LINES.iter().find(|(i, _, _)| *i == id)?;
    let n = usize::from(if summary { *b } else { *a });
    (n > 0).then_some(n)
}

/// A text wrapped to `width` columns in `n` lines when it can be:
/// narrower lines when it takes fewer, the natural wrap when more.
pub(crate) fn wrap_to(text: &str, width: usize, n: Option<usize>) -> Vec<String> {
    let natural = super::wrap(text, width);
    let Some(n) = n else { return natural };
    if natural.len() >= n {
        return natural;
    }
    for w in (1..width).rev() {
        let v = super::wrap(text, w);
        if v.len() == n {
            return v;
        }
        if v.len() > n {
            break;
        }
    }
    natural
}
