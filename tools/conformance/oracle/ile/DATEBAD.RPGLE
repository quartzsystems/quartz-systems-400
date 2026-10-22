**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 2: the identifier of an invalid date operation.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-s d date;
dcl-s t char(10) inz('2026-13-01');
monitor;
  d = %date(t : *iso);
on-error;
  QORACLE('DATEBAD' : 'status ' + %char(%status));
endmon;
*inlr = *on;
