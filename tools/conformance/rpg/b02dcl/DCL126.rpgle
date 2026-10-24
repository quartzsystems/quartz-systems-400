**FREE
//; dds: ORDF
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds k extname('ORDF' : *key) qualified end-ds;
dcl-ds r extname('ORDF') prefix(o_) end-ds;
dcl-ds xd extname('ORDF') prefix('XX' : 2) end-ds;
dcl-ds e extname('ORDF') qualified;
  myitem extfld('ITEM');
  price extfld inz(1.5);
end-ds;
o_item = 'widget';
xxdcus = 5;
e.myitem = 'renamed';
line = %char(%size(k)) + ' ' + %char(%size(r)) + ' ' + o_item + ' ' + %char(xxdcus);
write QSYSPRT line;
line = e.myitem + ' ' + %char(e.price) + ' ' + %char(%size(e.myitem));
write QSYSPRT line;
*inlr = *on;
