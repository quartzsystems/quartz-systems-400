**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 3: the identifier of %SUBST past the end.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-s s char(6) inz('abcdef');
dcl-s r char(10);
dcl-s start int(10) inz(5);
monitor;
  r = %subst(s : start : 4);
on-error;
  QORACLE('SUBSTBAD' : 'status ' + %char(%status));
endmon;
*inlr = *on;
