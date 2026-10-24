**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
ctl-opt dftactgrp(*no);
dcl-f custf keyed infds(fb);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds fb;
  fname *file;
  fopen ind pos(9);
  feof ind pos(10);
  fstat *status;
  fop *opcode;
  frec *record;
  ofname char(10) pos(83);
  ofrlen int(5) pos(125);
  ofmbr char(10) pos(129);
end-ds;
chain (102) custr;
line = fname + ' ' + fopen + ' ' + %char(fstat) + ' [' + fop + '] ' + %trim(frec);
write QSYSPRT line;
line = %trim(ofname) + ' ' + %char(ofrlen) + ' ' + %trim(ofmbr) + ' ' + %char(%size(fb));
write QSYSPRT line;
chain (999) custr;
line = %char(fstat) + ' [' + fop + ']';
write QSYSPRT line;
*inlr = *on;
