**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 4: a pointer advanced one byte past a static, then past
// an automatic, and dereferenced.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-pr auto end-pr;
dcl-s s1 char(4) inz('AAAA');
dcl-s s2 char(4) inz('BBBB');
dcl-s p pointer;
dcl-s v char(1) based(p);
dcl-s got char(1);
monitor;
  p = %addr(s1) + 4;
  got = v;
  QORACLE('PTRSTAT' : 'static neighbour read ok');
on-error;
  QORACLE('PTRSTAT' : 'static neighbour status ' + %char(%status));
endmon;
auto();
*inlr = *on;

dcl-proc auto;
  dcl-s a1 char(4) inz('CCCC');
  dcl-s a2 char(4) inz('DDDD');
  dcl-s q pointer;
  dcl-s w char(1) based(q);
  dcl-s g char(1);
  monitor;
    q = %addr(a1) + 4;
    g = w;
    QORACLE('PTRSTAT' : 'auto neighbour read ok');
  on-error;
    QORACLE('PTRSTAT' : 'auto neighbour status ' + %char(%status));
  endmon;
end-proc;
