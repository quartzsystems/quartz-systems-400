**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s t time inz(t'23.30.00');
dcl-s u time(*usa) inz(t'14.00.00');
line = %char(t + %hours(1)) + ' ' + %char(t + %minutes(45)) + ' ' + %char(t -
      %seconds(3601));
write QSYSPRT line;
line = %char(t + %hours(25)) + ' ' + %char(t - %hours(24)) + ' ' + %char(u +
      %minutes(90));
write QSYSPRT line;
line = %char(%diff(t'14.00.00':t'09.30.15':*hours)) + ' ' +
      %char(%diff(t'14.00.00':t'09.30.15':*minutes));
write QSYSPRT line;
line = %char(%diff(t'14.00.00':t'09.30.15':*seconds)) + ' ' +
      %char(%diff(t'09.00.00':u:*hours));
write QSYSPRT line;
line = %char(%diff(t'09.00.00':t'14.00.01':*h)) + ' ' +
      %char(%diff(t'09.00.00':t'14.00.01':*mn));
write QSYSPRT line;
line = %char(%subdt(t:*hours)) + ' ' + %char(%subdt(t:*minutes)) + ' ' +
      %char(%subdt(u:*seconds));
write QSYSPRT line;
line = %char(%subdt(u:*h)) + ' ' + %char(%subdt(t:*mn)) + ' ' + %char(%subdt(t:*s));
write QSYSPRT line;
t += %seconds(59);
line = %char(t);
write QSYSPRT line;
*inlr = *on;
