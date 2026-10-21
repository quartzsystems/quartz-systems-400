**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 9: reply R retries the failing instruction.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-s z packed(3:0) inz(7);
dcl-s d packed(3:0) inz(0);
// Reply R at the inquiry after CHGVAR'ing nothing: the divisor stays
// zero, so a retry fails again; a resume continues with z unchanged.
z = z / d;
QORACLE('REPLYR' : 'after, z=' + %char(z));
*inlr = *on;
