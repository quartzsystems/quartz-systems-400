// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SEU's print (`OPTION(6)`, `Print member` on Exit): the member's
//! records with their sequence numbers and dates to a spooled file of
//! `QSYSPRT`.

use spool::{Control, PageFormat, Printer};
use stg::Client;
use stg::proto::SrcMember;

use super::Ctx;
use crate::command::Fail;

/// Print an opened member.
pub fn member(ctx: &mut Ctx<'_>, m: &SrcMember) -> Result<String, Fail> {
    records(ctx, &m.member, &m.records)?;
    Ok(format!("Member {} printed.", m.member))
}

/// Print records under a heading naming the member.
pub fn records(ctx: &mut Ctx<'_>, member: &str, recs: &[(u32, u32, String)]) -> Result<(), Fail> {
    let link = &ctx.host.link;
    let (id, _) = link
        .borrow_mut()
        .with(|l| l.prt_open("QSYSPRT".into(), "*LIBL".into(), "*DFTACTGRP".into()))
        .map_err(|e| Fail::new("CPF4101", e))?;
    let fmt = PageFormat::default();
    let mut p = Printer::new(fmt);
    let head = format!(
        "SEU source listing   {}/{}({member})",
        ctx.at.0, ctx.at.1
    );
    p.write_str(&head, Control { skip_before: 1, ..Control::line() });
    p.write_str("", Control::line());
    for (seq, date, data) in recs {
        let text = format!("{}  {data:<80}  {date:06}", seu::seq_text(*seq));
        p.write_str(&text, Control::line());
    }
    let n = u32::try_from(recs.len()).unwrap_or(u32::MAX);
    let pages = n / u32::from(fmt.ovrflw.max(1)) + 1;
    let data = p.take();
    link.borrow_mut()
        .with(|l| l.splf_put(id, data, pages, n))
        .map_err(|e| Fail::new("CPF4101", e))?;
    link.borrow_mut()
        .with(|l| l.splf_close(id, false))
        .map_err(|e| Fail::new("CPF4101", e))?;
    Ok(())
}
