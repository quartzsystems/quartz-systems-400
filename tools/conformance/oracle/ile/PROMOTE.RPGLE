**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 11: a condition handler that promotes.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-pr CEEHDLR;
  proc pointer(*proc);
  token pointer;
  fc char(12) options(*omit);
  end-pr;
dcl-pr HANDLER;
  tok char(12);
  utok pointer;
  rc int(10);
  newtok char(12);
  end-pr;
dcl-s z packed(3:0);
dcl-s zero packed(3:0) inz(0);
dcl-s tokvar int(10) inz(1);
dcl-s ptok pointer;
dcl-s pp pointer(*proc);
pp = %paddr(HANDLER);
ptok = %addr(tokvar);
CEEHDLR(pp : ptok : *omit);
monitor;
  z = z / zero;
on-error;
  QORACLE('PROMOTE' : 'monitor saw status ' + %char(%status));
endmon;
*inlr = *on;

dcl-proc HANDLER;
  dcl-pi *n;
    tok char(12);
    utok pointer;
    rc int(10);
    newtok char(12);
    end-pi;
  // 30: promote to a user condition (facility USR, number 1).
  newtok = x'0003000101' + 'USR' + x'00000000';
  rc = 30;
end-proc;
