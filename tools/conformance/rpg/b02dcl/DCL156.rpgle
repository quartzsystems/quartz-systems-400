**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds vr;
  v2 varchar(6) inz('abc');
  v4 varchar(6 : 4) inz('de');
  ln2 uns(5) pos(1);
end-ds;
dcl-ds raw;
  pre int(5) inz(2);
  dat char(4) inz('wxyz');
  vv varchar(4) pos(1);
end-ds;
line = %char(ln2) + ' ' + %char(%size(vr)) + ' ' + %char(%addr(v4) - %addr(vr)) + ' [' +
  vv + '] ' + %char(%len(vv));
write QSYSPRT line;
*inlr = *on;
