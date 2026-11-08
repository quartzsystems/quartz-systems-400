// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 9 QEMU steps: RPG compiled and run on the booted system.

use crate::phase5::secofr_command_entry;
use crate::phase6::{entry_wait, has, sign_off};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// A program with a subprocedure, a loop and a data structure, that
/// says what it computed with DSPLY.
const RPGHELLO: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-ds totals qualified inz;
  sum packed(7:0);
  count int(10);
end-ds;
dcl-s i int(10);
dcl-s msg char(40);
for i = 1 to 10;
  totals.sum += i;
  totals.count += 1;
endfor;
msg = 'Hello from RPG, total ' + %char(totals.sum) + ' of ' + %char(totals.count);
dsply msg;
msg = 'squared ' + %char(square(12));
dsply msg;
*inlr = *on;
dcl-proc square;
  dcl-pi *n int(10);
    n int(10) value;
  end-pi;
  return n * n;
end-proc;
";

/// Embedded SQL: a table made, filled and read back.
const RPGSQL: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-s n int(10);
dcl-s total packed(9:2);
dcl-s msg char(40);
exec sql set option commit = *none;
exec sql drop table rpgtest/parts;
exec sql create table rpgtest/parts (id int, price decimal(7,2));
exec sql insert into rpgtest/parts values (1, 2.50), (2, 7.25), (3, 10.00);
exec sql select count(*), sum(price) into :n, :total from rpgtest/parts;
msg = 'SQL rows ' + %char(n) + ' total ' + %char(total) + ' code ' + %char(sqlcode);
dsply msg;
*inlr = *on;
";

/// A module exporting a procedure, for a service program.
const RPGSRV: &str = "**FREE
ctl-opt nomain;
dcl-proc greet export;
  dcl-pi *n varchar(30);
    who varchar(10) const;
  end-pi;
  return 'Bound hello, ' + who;
end-proc;
";

/// A module calling the service program's procedure.
const RPGUSE: &str = "**FREE
dcl-pr greet varchar(30);
  who varchar(10) const;
end-pr;
dcl-s msg char(40);
msg = greet('Quartz');
dsply msg;
*inlr = *on;
";

/// Step 158's keyed DDS file: an alias and a null-capable field.
const CUSTPF: &str = "     A          R CUSTR
     A            CUSTNO         5P 0
     A            CNAME         20A         ALIAS(CUSTOMER_NAME)
     A            NOTE          10A         ALWNULL
     A          K CUSTNO
";

/// Step 158's setup at the service console, after the DDS member.
const KEYED_SETUP: &[&str] = &[
    "crtpf FILE(RPGKEY/CUSTPF) SRCFILE(RPGKEY/QDDSSRC)",
    "runsql SQL('INSERT INTO RPGKEY.CUSTPF VALUES(1, ''Alpha'', NULL), (2, ''Beta'', ''vip'')') COMMIT(*NONE)",
    "runsql SQL('CREATE TABLE RPGKEY.ORDERS (ORDER_ID INTEGER NOT NULL PRIMARY KEY, AMOUNT DECIMAL(7, 2) NOT NULL, MEMO VARCHAR(20))') COMMIT(*NONE)",
    "runsql SQL('INSERT INTO RPGKEY.ORDERS VALUES(7, 12.50, NULL), (8, 3.25, ''rush'')') COMMIT(*NONE)",
];

/// CHAIN by key into both files, a field by its alias, null indicators.
const RPGKEYED: &str = "**FREE
ctl-opt dftactgrp(*no) alwnull(*usrctl);
dcl-f CUSTPF keyed alias;
dcl-f ORDERS keyed rename(ORDERS:ORDERR);
dcl-s i int(10);
dcl-s cust packed(5);
dcl-s oid int(10);
dcl-s msg char(52);
for i = 1 to 2;
  cust = i;
  chain cust CUSTPF;
  if not %found(CUSTPF);
    msg = 'KEYC' + %char(cust) + ' not found';
  elseif %nullind(NOTE);
    msg = 'KEYC' + %char(cust) + ' ' + %trim(CUSTOMER_NAME) + ' note null';
  else;
    msg = 'KEYC' + %char(cust) + ' ' + %trim(CUSTOMER_NAME) + ' note ' + %trim(NOTE);
  endif;
  dsply msg;
endfor;
for oid = 7 to 9;
  chain (oid) ORDERS;
  if not %found(ORDERS);
    msg = 'KEYO' + %char(oid) + ' not found';
  elseif %nullind(MEMO);
    msg = 'KEYO' + %char(oid) + ' ' + %char(AMOUNT) + ' memo null';
  else;
    msg = 'KEYO' + %char(oid) + ' ' + %char(AMOUNT) + ' memo ' + %trim(MEMO);
  endif;
  dsply msg;
endfor;
*inlr = *on;
";

impl Suite {
    /// Step 158: RPG compiled on the system against externally described
    /// keyed files: a DDS PF with an alias and a null-capable field and
    /// an SQL table keyed on its primary key. The program CHAINs both by
    /// key, reads the alias and the null indicators, and its level
    /// checks pass at open.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn rpg_keyed_files(&self, s: &mut Session) -> Result<String, String> {
        let lib = s.command("crtlib LIB(RPGKEY) TEXT('Step 158')", CMD)?;
        if !lib.contains("RPGKEY") {
            return Err(format!("crtlib RPGKEY: {lib}"));
        }
        for (file, len) in [("QRPGLESRC", 112), ("QDDSSRC", 92)] {
            let made = s.command(&format!("crtsrcpf FILE(RPGKEY/{file}) RCDLEN({len})"), CMD)?;
            if !made.contains("created") {
                return Err(format!("crtsrcpf {file}: {made}"));
            }
        }
        put_in(s, "RPGKEY/QDDSSRC", "CUSTPF", "PF", CUSTPF)?;
        for line in KEYED_SETUP {
            let out = s.command(line, CMD)?;
            if out
                .lines()
                .any(|l| l.trim_start().starts_with("CPF") || l.contains("SQL0"))
            {
                return Err(format!("{line}:\n{out}"));
            }
        }
        put_in(s, "RPGKEY/QRPGLESRC", "RPGKEYED", "RPGLE", RPGKEYED)?;
        secofr_command_entry(s)?;
        entry_wait(s, "addlible RPGKEY", has(&["RPGKEY"]))?;
        entry_wait(
            s,
            "crtbndrpg PGM(RPGKEY/RPGKEYED) SRCFILE(RPGKEY/QRPGLESRC)",
            has(&["Program RPGKEYED created"]),
        )?;
        let called = entry_wait(s, "call RPGKEY/RPGKEYED", has(&["KEYO9 not found"]))?;
        for want in [
            "KEYC1 Alpha note null",
            "KEYC2 Beta note vip",
            "KEYO7 12.50 memo null",
            "KEYO8 3.25 memo rush",
        ] {
            if !called.contains(want) {
                return Err(format!("{want} is missing:\n{called}"));
            }
        }
        sign_off(s)?;
        Ok("CRTBNDRPG compiled CHAINs into a keyed DDS PF and an SQL table keyed on its primary key; the call found each key, read CUSTOMER_NAME by its alias and the null indicators of NOTE and MEMO, and opened both files past their level checks".into())
    }

    /// Step 56: RPG from a source member: `CRTBNDRPG` compiles it, the
    /// listing is spooled, `CALL` runs it and its DSPLY lines show; a
    /// member in error is not created .
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn rpg_program(&self, s: &mut Session) -> Result<String, String> {
        secofr_command_entry(s)?;
        entry_wait(s, "crtlib LIB(RPGTEST)", has(&["RPGTEST", "created"]))?;
        entry_wait(
            s,
            "crtsrcpf FILE(RPGTEST/QRPGLESRC) RCDLEN(112)",
            has(&["QRPGLESRC", "created"]),
        )?;
        put(s, "RPGHELLO", RPGHELLO)?;
        entry_wait(
            s,
            "crtbndrpg PGM(RPGTEST/RPGHELLO) SRCFILE(RPGTEST/QRPGLESRC)",
            has(&["Program RPGHELLO created"]),
        )?;
        let called = entry_wait(
            s,
            "call RPGTEST/RPGHELLO",
            has(&["Hello from RPG, total 55 of 10"]),
        )?;
        if !called.contains("squared 144") {
            return Err(format!("the subprocedure's line is missing:\n{called}"));
        }
        put(
            s,
            "RPGBAD",
            "**FREE\ndcl-s a int(10);\na = nosuch;\n*inlr = *on;\n",
        )?;
        entry_wait(
            s,
            "crtbndrpg PGM(RPGTEST/RPGBAD) SRCFILE(RPGTEST/QRPGLESRC)",
            has(&["Program RPGBAD was not created"]),
        )?;
        sign_off(s)?;
        Ok("CRTBNDRPG compiled RPGHELLO; CALL displayed the loop's total and the subprocedure's result; a member in error was not created".into())
    }

    /// Step 57: embedded SQL: `CRTSQLRPGI` precompiles and compiles, and
    /// the program creates, fills and reads a table.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn rpg_sql(&self, s: &mut Session) -> Result<String, String> {
        secofr_command_entry(s)?;
        put(s, "RPGSQL", RPGSQL)?;
        entry_wait(
            s,
            "crtsqlrpgi OBJ(RPGTEST/RPGSQL) SRCFILE(RPGTEST/QRPGLESRC) COMMIT(*NONE)",
            has(&["Program RPGSQL created"]),
        )?;
        entry_wait(
            s,
            "call RPGTEST/RPGSQL",
            has(&["SQL rows 3 total 19.75 code 0"]),
        )?;
        sign_off(s)?;
        Ok("CRTSQLRPGI built RPGSQL; it made RPGTEST/PARTS, inserted three rows and read the count and sum back with SQLCODE 0".into())
    }

    /// Step 58: a module, a service program and a program bound to it.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn rpg_service_program(&self, s: &mut Session) -> Result<String, String> {
        secofr_command_entry(s)?;
        put(s, "RPGSRV", RPGSRV)?;
        put(s, "RPGUSE", RPGUSE)?;
        entry_wait(
            s,
            "crtrpgmod MODULE(RPGTEST/RPGSRV) SRCFILE(RPGTEST/QRPGLESRC)",
            has(&["Module RPGSRV created"]),
        )?;
        entry_wait(
            s,
            "crtsrvpgm SRVPGM(RPGTEST/RPGSRV) MODULE(RPGTEST/RPGSRV) EXPORT(*ALL)",
            has(&["RPGTEST/RPGSRV created"]),
        )?;
        entry_wait(
            s,
            "crtrpgmod MODULE(RPGTEST/RPGUSE) SRCFILE(RPGTEST/QRPGLESRC)",
            has(&["Module RPGUSE created"]),
        )?;
        entry_wait(
            s,
            "crtpgm RPGTEST/RPGUSE MODULE(RPGTEST/RPGUSE) BNDSRVPGM(RPGTEST/RPGSRV)",
            has(&["RPGTEST/RPGUSE created"]),
        )?;
        entry_wait(s, "call RPGTEST/RPGUSE", has(&["Bound hello, Quartz"]))?;
        sign_off(s)?;
        Ok("CRTRPGMOD made two modules; CRTSRVPGM and CRTPGM bound them; the call reached the service program's procedure".into())
    }
}

/// A source member into `RPGTEST/QRPGLESRC` over the service console.
fn put(s: &mut Session, name: &str, text: &str) -> Result<(), String> {
    put_in(s, "RPGTEST/QRPGLESRC", name, "RPGLE", text)
}

/// A source member of type `ty` into `file` over the service console.
fn put_in(s: &mut Session, file: &str, name: &str, ty: &str, text: &str) -> Result<(), String> {
    let buf = format!("{name}SRC");
    crate::qir::qemu::push(s, &buf, text)?;
    let r = s.command(
        &format!("svc.srcput {file} {name} SRCTYPE({ty}) FROMSTMF({buf})"),
        CMD,
    )?;
    if !r.contains("written") {
        return Err(format!("srcput {name}: {r}"));
    }
    Ok(())
}
