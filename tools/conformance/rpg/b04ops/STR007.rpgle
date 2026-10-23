**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s c char(20) inz('hello world');
dcl-s v varchar(30) inz('a.b.c');
line = 'a [' + %scanrpl('o':'0':c) + ']';
write QSYSPRT line;
line = 'b [' + %scanrpl('l':'LL':c) + ']';
write QSYSPRT line;
line = 'c [' + %scanrpl('o':'':c) + ']';
write QSYSPRT line;
line = 'd [' + %scanrpl('o':'*':c:6) + ']';
write QSYSPRT line;
line = 'e [' + %scanrpl('l':'_':c:1:4) + ']';
write QSYSPRT line;
line = 'f [' + %scanrpl('.':'::':v) + '] ' + %char(%len(%scanrpl('.':'::':v)));
write QSYSPRT line;
line = 'g [' + %scanrpl('zz':'y':v) + ']';
write QSYSPRT line;
line = 'h ' + %char(%scanr('o':c)) + ' ' + %char(%scanr('l':c)) + ' ' +
      %char(%scanr('z':c));
write QSYSPRT line;
line = 'i ' + %char(%scanr('l':c:1:5)) + ' ' + %char(%scanr('o':c:6));
write QSYSPRT line;
line = 'j ' + %char(%scanr('.':v)) + ' ' + %char(%scanr('ll':c));
write QSYSPRT line;
*inlr = *on;
