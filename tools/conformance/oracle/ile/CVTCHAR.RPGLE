**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 1: %CHAR of decimals with decimal positions.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-s a packed(5:2) inz(1.50);
dcl-s b packed(5:2) inz(0.5);
dcl-s c packed(5:0) inz(100);
dcl-s n packed(5:2) inz(-0.5);
QORACLE('CVTCHAR' : '[' + %char(a) + ']');
QORACLE('CVTCHAR' : '[' + %char(b) + ']');
QORACLE('CVTCHAR' : '[' + %char(c) + ']');
QORACLE('CVTCHAR' : '[' + %char(n) + ']');
*inlr = *on;
