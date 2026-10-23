**FREE
ctl-opt dftactgrp(*no) datfmt(*dmy.) timfmt(*hms,);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s d date inz(d'05.03.26');
dcl-s t time inz(t'14,30,05');
dcl-s e date(*iso);
dcl-ds raw;
  rd date inz(d'05.03.26');
  rt time inz(t'14,30,05');
end-ds;
line = %char(d) + ' ' + %char(t) + ' ' + raw;
write QSYSPRT line;
e = %date('05.03.26');
line = %char(e) + ' ' + %char(%date(050326)) + ' ' + %char(%time('14,31,06'));
write QSYSPRT line;
line = %char(%dec(d)) + ' ' + %char(%dec(t));
write QSYSPRT line;
*inlr = *on;
