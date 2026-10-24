**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s u1 char(10) inz(*user);
dcl-s u2 varchar(18) inz(*user);
dcl-s k1 ind;
dcl-s k2 ind;
k1 = %len(%trim(u1)) > 0;
k2 = %trim(u1) = u2;
line = k1 + ' ' + k2;
write QSYSPRT line;
*inlr = *on;
