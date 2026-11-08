// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `CRTPGM` and `CRTSRVPGM` report, as `compdiag` values: the
//! binder's diagnostics when it made the object, or the error that
//! stopped it. The binder prints no listing; its report is the
//! command's output, one line each ([`report_lines`]).

use compdiag::{Diagnostic, Help, Where};

use super::{Diag, Severity};
use crate::IleError;

/// The binder's diagnostics as the command prints them: `ID text`.
#[must_use]
pub fn report_lines(diags: &[Diag]) -> Vec<String> {
    diags
        .iter()
        .map(|d| format!("{} {}", d.id, d.text))
        .collect()
}

/// The diagnostics of a bind: those it made the object with, or the
/// error that stopped it. A binder source error points into the source
/// member (`source`); every other is about no source line.
#[must_use]
pub fn diagnostics(
    outcome: Result<&[Diag], &IleError>,
    source: Option<&Where>,
    help: Help<'_>,
) -> Vec<Diagnostic> {
    match outcome {
        Ok(diags) => diags
            .iter()
            .map(|d| Diagnostic {
                severity: match d.severity {
                    Severity::Info => 0,
                    Severity::Warning => 10,
                    Severity::Error => 30,
                },
                help: help(&d.id, &[]),
                id: d.id.clone(),
                text: d.text.clone(),
                ..Diagnostic::default()
            })
            .collect(),
        Err(IleError::BinderSource { line, msg }) => vec![Diagnostic {
            at: source.cloned().unwrap_or_default(),
            line: if source.is_some() { *line } else { 0 },
            severity: 30,
            // The binder language's errors carry no identifier here.
            text: msg.clone(),
            ..Diagnostic::default()
        }],
        Err(e) => {
            let text = e.to_string();
            let id = match e {
                IleError::Unresolved(_) => "CPD5D02",
                IleError::NoSuchExport(_) => "CPD5D03",
                IleError::NoEntry => "CPD5D06",
                _ => "",
            };
            vec![Diagnostic {
                severity: 30,
                help: if id.is_empty() {
                    String::new()
                } else {
                    help(id, &[])
                },
                id: id.to_string(),
                text: text
                    .strip_prefix(&format!("{id}: "))
                    .unwrap_or(&text)
                    .to_string(),
                ..Diagnostic::default()
            }]
        }
    }
}
