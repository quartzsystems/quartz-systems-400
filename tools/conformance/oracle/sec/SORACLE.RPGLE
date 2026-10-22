**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Writes one observation line, "tag: text", to SECORA/ORACLE.
ctl-opt dftactgrp(*no) actgrp(*caller) option(*srcstmt);
dcl-f ORACLE disk(80) usage(*output) extfile('SECORA/ORACLE');
dcl-pi *n; tag char(10); line char(80); end-pi;
dcl-ds rec len(80); end-ds;
rec = %trimr(tag) + ': ' + line;
write ORACLE rec;
return;
