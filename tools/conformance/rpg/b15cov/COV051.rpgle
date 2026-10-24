**FREE
ctl-opt dftactgrp(*no) alwnull(*usrctl);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(5) nullind inz('abc');
dcl-s n packed(5:0) nullind;
%nullind(c) = *on;
line = 'c ' + %nullind(c) + ' ' + %nullind(n);
write QSYSPRT line;
%nullind(c) = *off;
n = 5;
line = 'c ' + %nullind(c) + ' [' + c + '] ' + %char(n);
write QSYSPRT line;
*inlr = *on;
