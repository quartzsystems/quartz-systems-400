**FREE
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f ordf keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds a likerec(ordr);
dcl-ds k likerec(ordr : *key);
dcl-ds i likerec(ordr : *input);
dcl-ds o likerec(ordr : *output);
read ordr i;
k.ordcus = i.ordcus;
k.ordno = i.ordno;
chain %kds(k) ordr a;
line = %char(%size(a)) + ' ' + %char(%size(k)) + ' ' + %char(%size(i)) + ' ' +
  %char(%size(o));
write QSYSPRT line;
line = %char(a.ordcus) + ' ' + %char(a.ordno) + ' ' + %trim(a.item);
write QSYSPRT line;
*inlr = *on;
