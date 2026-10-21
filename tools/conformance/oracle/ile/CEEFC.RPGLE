**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 12: the feedback codes of misused condition APIs.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE'); name char(10); line char(80); end-pr;
dcl-pr CEEHDLU; proc pointer(*proc); fc char(12) options(*omit); end-pr;
dcl-pr CEEMRCR; action int(10); fc char(12) options(*omit); end-pr;
dcl-pr NEVER; tok char(12); utok pointer; rc int(10); newtok char(12); end-pr;
dcl-s fc char(12);
dcl-s pp pointer(*proc);
dcl-s one int(10) inz(1);
dcl-s hex char(24);
pp = %paddr(NEVER);
CEEHDLU(pp : fc);
QORACLE('CEEFC' : 'CEEHDLU unregistered: ' + %subst(fc : 6 : 3) + %char(%int(%subst(fc : 3 : 2))));
CEEMRCR(one : fc);
QORACLE('CEEFC' : 'CEEMRCR action 1: ' + %subst(fc : 6 : 3) + %char(%int(%subst(fc : 3 : 2))));
*inlr = *on;

dcl-proc NEVER;
  dcl-pi *n; tok char(12); utok pointer; rc int(10); newtok char(12); end-pi;
  rc = 20;
end-proc;
