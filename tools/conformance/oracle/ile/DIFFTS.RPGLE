**free
// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.
// Open question 6: %DIFF in months between timestamps a month apart on
// the same day, once with an earlier time of day and once with a later.
ctl-opt dftactgrp(*no) actgrp(*new) option(*srcstmt);
dcl-pr QORACLE extpgm('QORACLE');
  name char(10) const;
  line char(80) const;
  end-pr;
dcl-s t1 timestamp inz(z'2026-01-15-10.00.00.000000');
dcl-s t2 timestamp inz(z'2026-02-15-09.00.00.000000');
dcl-s t3 timestamp inz(z'2026-02-15-11.00.00.000000');
QORACLE('DIFFTS' : 'earlier time of day: months '
                   + %char(%diff(t2 : t1 : *months)));
QORACLE('DIFFTS' : 'later time of day: months '
                   + %char(%diff(t3 : t1 : *months)));
*inlr = *on;
