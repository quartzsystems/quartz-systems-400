**FREE
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f ORDF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c packed(5:0) inz(101);
dcl-ds kds likerec(ORDR:*KEY);
// A partial key, then the full key, then %KDS.
setll c ORDF;
reade c ORDF;
dow not %eof(ORDF);
line = %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + ITEM + %char(QTY) + ' ' + %char(PRICE);
write QSYSPRT line;
reade c ORDR;
enddo;
setll (104:2) ORDR;
reade (104) ORDF;
line = 'next ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' eof=' + %eof(ORDF);
write QSYSPRT line;
reade (104) ORDF;
line = 'then eof=' + %eof(ORDF);
write QSYSPRT line;
kds.ORDCUS = 107;
kds.ORDNO = 2;
chain %kds(kds) ORDF;
line = 'kds ' + ITEM + %char(QTY) + ' ' + %found(ORDF);
write QSYSPRT line;
setll %kds(kds:1) ORDF;
read ORDF;
line = 'kds1 ' + %char(ORDCUS) + '/' + %char(ORDNO) + ' ' + ITEM;
write QSYSPRT line;
*inlr = *on;
