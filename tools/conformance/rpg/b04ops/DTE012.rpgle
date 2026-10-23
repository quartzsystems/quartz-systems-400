**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s z timestamp inz(z'2026-03-05-23.59.59.999999');
dcl-s y timestamp inz(z'2025-01-31-08.00.00.000000');
dcl-s r timestamp;
r = z + %mseconds(1);
line = %char(r) + ' ' + %char(z + %days(1));
write QSYSPRT line;
line = %char(y + %months(1)) + ' ' + %char(y - %years(1));
write QSYSPRT line;
line = %char(y + %hours(2)) + ' ' + %char(y - %minutes(1)) + ' ' + %char(y +
      %seconds(90));
write QSYSPRT line;
line = %char(y - %mseconds(1500000));
write QSYSPRT line;
line = %char(%diff(z:y:*ms)) + ' ' + %char(%diff(z:y:*seconds)) + ' ' +
      %char(%diff(z:y:*minutes));
write QSYSPRT line;
line = %char(%diff(z:y:*hours)) + ' ' + %char(%diff(z:y:*days)) + ' ' +
      %char(%diff(z:y:*months));
write QSYSPRT line;
line = %char(%diff(z:y:*years)) + ' ' + %char(%diff(y:z:*days)) + ' ' +
      %char(%diff(y:z:*mseconds));
write QSYSPRT line;
line = %char(%subdt(z:*years)) + ' ' + %char(%subdt(z:*months)) + ' ' +
      %char(%subdt(z:*days));
write QSYSPRT line;
line = %char(%subdt(z:*hours)) + ' ' + %char(%subdt(z:*minutes)) + ' ' +
      %char(%subdt(z:*seconds));
write QSYSPRT line;
line = %char(%subdt(z:*mseconds)) + ' ' + %char(%subdt(z:*ms)) + ' ' +
      %char(%subdt(y:*d));
write QSYSPRT line;
*inlr = *on;
