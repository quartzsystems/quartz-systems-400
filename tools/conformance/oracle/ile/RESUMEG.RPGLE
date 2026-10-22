**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 7: reply G after a divide by zero.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-s z packed(3:0) inz(7);
dcl-s d packed(3:0) inz(0);
z = z / d;
QORACLE('RESUMEG' : 'after, z=' + %char(z));
*inlr = *on;
