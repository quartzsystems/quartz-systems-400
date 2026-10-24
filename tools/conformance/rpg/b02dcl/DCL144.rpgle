**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds rec qualified;
  amt packed(9:3);
  nm char(7);
end-ds;
dcl-s arr int(5) dim(4);
dcl-s dt date(*eur);
dcl-s a like(rec.amt);
dcl-s b like(rec.nm : +1);
dcl-s c like(arr);
dcl-s d like(dt) inz(d'2026-02-28');
dcl-s e like(arr) dim(2);
line = %char(%len(a)) + ' ' + %char(%decpos(a)) + ' ' + %char(%len(b)) + ' ' +
  %char(%size(c)) + ' ' + %char(d) + ' ' + %char(%elem(e));
write QSYSPRT line;
*inlr = *on;
