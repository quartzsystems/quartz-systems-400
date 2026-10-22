**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 10: which MONMSG CPF9999 runs, the callee's or the caller's.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-pr FCCALLEE extpgm('FCCALLEE');
  end-pr;
monitor;
  FCCALLEE();
on-error;
  QORACLE('FCWHERE' : 'caller saw status ' + %char(%status));
endmon;
*inlr = *on;
