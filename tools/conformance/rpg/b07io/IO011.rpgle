**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: ORDF
//; sqlfile: ORDF
ctl-opt dftactgrp(*no);
dcl-f ORDF keyed;
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds all extname('CUSTF') end-ds;
dcl-ds keys extname('CUSTF':*KEY) qualified end-ds;
dcl-ds ords extname('ORDF':'ORDR':*INPUT) prefix(X_) end-ds;
// EXTNAME data structures: *ALL, *KEY, a format and *INPUT.
CUSNO = 101;
NAME = 'Nobody';
line = %char(%size(all)) + ' ' + %char(%size(keys)) + ' ' + %char(%size(ords));
write QSYSPRT line;
line = %subst(all:4:6) + '|' + %char(CUSNO);
write QSYSPRT line;
keys.CUSNO = 104;
chain %kds(keys) ORDF;
line = 'ord ' + ITEM + %char(QTY);
write QSYSPRT line;
read ORDF ords;
line = 'ds ' + X_ITEM + %char(X_QTY) + ' ' + %char(X_ORDCUS);
write QSYSPRT line;
*inlr = *on;
