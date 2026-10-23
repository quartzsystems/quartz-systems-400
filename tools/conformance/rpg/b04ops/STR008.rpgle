**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(12) inz('abcdefgh');
dcl-s v varchar(20) inz('xyz12');
dcl-s buf char(10) inz(x'C1C2C300C5C6C7C8C9D1');
dcl-s p pointer;
line = 'a [' + %left(c:3) + '] [' + %right(c:3) + ']';
write QSYSPRT line;
line = 'b [' + %left(v:2) + '] [' + %right(v:2) + ']';
write QSYSPRT line;
line = 'c [' + %right(%trimr(c):4) + '] ' + %char(%len(%left(c:5)));
write QSYSPRT line;
line = 'd ' + %char(%charcount(c)) + ' ' + %char(%charcount(v)) + ' ' +
      %char(%charcount(''));
write QSYSPRT line;
p = %addr(buf);
line = 'e [' + %str(p:10) + '] ' + %char(%len(%str(p:10)));
write QSYSPRT line;
line = 'f [' + %str(p:2) + ']';
write QSYSPRT line;
p = %addr(c);
%str(p:5) = 'pq';
line = 'g ' + %subst(c:1:2) + ' ' + %char(%len(%str(p:12)));
write QSYSPRT line;
line = 'h [' + %str(p:12) + ']';
write QSYSPRT line;
*inlr = *on;
