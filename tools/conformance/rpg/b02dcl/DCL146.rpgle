**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-c MAX 50;
dcl-c RATE 0.075;
dcl-c NAME 'Quartz';
dcl-c HEX x'F0F1';
dcl-c DAY d'2026-12-25';
dcl-c TIM t'10.20.30';
dcl-c STAMP z'2026-12-25-10.20.30.000000';
dcl-c ON *on;
dcl-c UNI u'00510053';
dcl-s buf char(MAX);
dcl-s arr int(5) dim(MAX);
dcl-s amt packed(7:3) inz(RATE);
line = %char(%size(buf)) + ' ' + %char(%elem(arr)) + ' ' + %char(amt) + ' ' + NAME + ' ' +
  HEX;
write QSYSPRT line;
line = %char(DAY) + ' ' + %char(TIM) + ' ' + %char(STAMP);
write QSYSPRT line;
line = %char(UNI) + ' ' + %char(%len(NAME)) + ' ' + %char(MAX * RATE);
write QSYSPRT line;
*inlr = *on;
