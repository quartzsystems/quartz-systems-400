**FREE
ctl-opt dftactgrp(*no) decedit('0,') cursym('#');
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s n packed(7:2) inz(0.5);
dcl-s m packed(9:2) inz(-12345.67);
line = '[' + %editc(n:'J') + '] [' + %editc(m:'1':*cursym) + ']';
write QSYSPRT line;
*inlr = *on;
