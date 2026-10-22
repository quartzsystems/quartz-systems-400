**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 8: %STATUS of each machine condition.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-s z packed(3:0);
dcl-s zero packed(3:0) inz(0);
dcl-s big packed(9:0) inz(123456);
dcl-s bad char(2) inz(x'FFFF');
dcl-s badp packed(3:0) based(pbad);
dcl-s arr int(10) dim(3);
dcl-s i int(10) inz(5);
dcl-s p pointer;
dcl-s v int(10) based(p);
monitor;
  z = z / zero;
on-error;
  QORACLE('STATMAP' : 'MCH1211 status ' + %char(%status));
endmon;
monitor;
  z = big;
on-error;
  QORACLE('STATMAP' : 'MCH1210 status ' + %char(%status));
endmon;
pbad = %addr(bad);
monitor;
  z = badp + 1;
on-error;
  QORACLE('STATMAP' : 'MCH1202 status ' + %char(%status));
endmon;
monitor;
  arr(i) = 1;
on-error;
  QORACLE('STATMAP' : 'MCH0603 status ' + %char(%status));
endmon;
monitor;
  v = 1;
on-error;
  QORACLE('STATMAP' : 'MCH3601 status ' + %char(%status));
endmon;
*inlr = *on;
