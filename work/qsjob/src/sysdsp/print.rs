// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Print key: the screen as it stands (with what was typed) is
//! spooled through printer file `QSYSPRT`, the Phase 5 path every
//! printer file takes, and the message line says so.

use mi::io::spool::{PrinterState, SpoolSink};
use stg::remote::spool::RemoteSpool;
use tn5250::ds::attr;
use tn5250::screen::Read;

use crate::console::host::Host;

/// The printed image: a rule above and below, the rows between.
#[must_use]
pub fn image(rows: &[String], header: &str) -> Vec<String> {
    let width = rows.first().map_or(80, String::len);
    let rule: String = (1..=width)
        .map(|i| match i % 10 {
            0 => char::from_digit(u32::try_from(i / 10 % 10).unwrap_or(0), 10).unwrap_or('0'),
            5 => '+',
            _ => '.',
        })
        .collect();
    let mut out = vec![header.to_string(), String::new(), rule.clone()];
    out.extend(rows.iter().map(|r| r.trim_end().to_string()));
    out.push(rule);
    out
}

/// Spool the lines to `QSYSPRT`; the spooled file's number.
fn spool(host: &Host, lines: &[String]) -> Result<u32, String> {
    let mut sink = RemoteSpool::new(host.link.clone());
    let open = sink
        .open("QSYSPRT", "*LIBL", "*DFTACTGRP")
        .map_err(|e| e.msgid)?;
    let id = open.id;
    let mut p = PrinterState::new(open);
    for l in lines {
        let bytes = ccsid::to_ebcdic(l).unwrap_or_default();
        p.write(&mut sink, "", &bytes).map_err(|e| e.msgid.clone())?;
    }
    p.flush(&mut sink).map_err(|e| e.msgid.clone())?;
    sink.close(id, false).map_err(|e| e.msgid)?;
    Ok(id)
}

/// Print the job's screen and say so on its message line.
pub fn screen(host: &Host) {
    let rows = {
        let d = host.display.borrow();
        let Some(d) = d.as_ref() else {
            return;
        };
        d.model.text_rows()
    };
    let header = format!(
        "Display {} printed for job {:06}/{}/{}",
        host.start.device, host.start.number, host.start.user, host.start.name
    );
    let message = match spool(host, &image(&rows, &header)) {
        Ok(_) => "Print request completed: the display is spooled to QSYSPRT.".to_string(),
        Err(id) => format!("{id} The display could not be printed."),
    };
    let mut d = host.display.borrow_mut();
    let Some(d) = d.as_mut() else {
        return;
    };
    let last = d.model.rows();
    if !d.model.fields().iter().any(|f| f.row == last) {
        d.model.set_error_row(last);
        d.model.error_line(&message, attr::WHITE);
    }
    d.flush(Read::MdtFields);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_has_rules() {
        let rows = vec![" Main".to_string() + &" ".repeat(75), " ".repeat(80)];
        let img = image(&rows, "head");
        assert_eq!(img.len(), 6);
        assert_eq!(img[2].len(), 80);
        assert_eq!(&img[2][..10], "....+....1");
        assert_eq!(img[3], " Main");
    }
}
