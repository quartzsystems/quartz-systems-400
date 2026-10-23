**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s asc packed(3:0) dim(5) ascend;
dcl-s chr char(3) dim(4);
dcl-s taba char(3) dim(3) ctdata perrcd(1);
dcl-s tabb char(4) dim(3) alt(taba);
dcl-s tabn packed(3:0) dim(4) ascend ctdata perrcd(1);
asc(1) = 10;
asc(2) = 20;
asc(3) = 30;
asc(4) = 40;
asc(5) = 50;
chr(1) = 'a';
chr(2) = 'b';
chr(3) = 'c';
chr(4) = 'b';
line = 'a ' + %char(%lookup(30:asc)) + ' ' + %char(%lookup(35:asc)) + ' ' +
      %char(%lookup('b':chr));
write QSYSPRT line;
line = 'b ' + %char(%lookup('b':chr:3)) + ' ' + %char(%lookup('b':chr:1:1)) + ' ' +
      %char(%lookup('a':chr:2));
write QSYSPRT line;
line = 'c ' + %char(%lookuplt(30:asc)) + ' ' + %char(%lookuple(30:asc)) + ' ' +
      %char(%lookuplt(35:asc));
write QSYSPRT line;
line = 'd ' + %char(%lookupgt(30:asc)) + ' ' + %char(%lookupge(30:asc)) + ' ' +
      %char(%lookupge(35:asc));
write QSYSPRT line;
line = 'e ' + %char(%lookuplt(5:asc)) + ' ' + %char(%lookupgt(50:asc)) + ' ' +
      %char(%lookupge(5:asc));
write QSYSPRT line;
line = 'f ' + %char(%lookupgt(10:asc:3)) + ' ' + %char(%lookuple(45:asc:1:2));
write QSYSPRT line;
line = 'g ' + taba + ' ' + tabb;
write QSYSPRT line;
if %tlookup('BBB':taba:tabb);
line = 'h ' + taba + ' ' + tabb;
write QSYSPRT line;
endif;
if not %tlookup('ZZZ':taba);
line = 'i ' + taba + ' ' + tabb;
write QSYSPRT line;
endif;
if %tlookupgt(25:tabn);
line = 'j ' + %char(tabn);
write QSYSPRT line;
endif;
if %tlookuple(25:tabn);
line = 'k ' + %char(tabn);
write QSYSPRT line;
endif;
*inlr = *on;
**ctdata taba
AAAone
BBBtwo
CCCsix
**ctdata tabn
010
020
030
040
