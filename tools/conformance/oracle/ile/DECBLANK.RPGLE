**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 5: %DEC of text with an embedded blank.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-s t char(5) inz('1 2');
dcl-s n packed(5:0);
monitor;
  n = %dec(t : 5 : 0);
  QORACLE('DECBLANK' : '[' + %char(n) + ']');
on-error;
  QORACLE('DECBLANK' : 'status ' + %char(%status));
endmon;
*inlr = *on;
