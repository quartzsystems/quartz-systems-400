**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 14: the identifier a stack overflow raises.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-pr DOWN; n int(10) value; end-pr;
dcl-ds psds psds;
  excid char(7) pos(40);
end-ds;
monitor;
  DOWN(1);
on-error;
  QORACLE('DEEPREC' : 'got ' + excid + ' status ' + %char(%status));
endmon;
*inlr = *on;

dcl-proc DOWN;
  dcl-pi *n; n int(10) value; end-pi;
  dcl-s pad char(4096);
  pad = %char(n);
  DOWN(n + 1);
end-proc;
