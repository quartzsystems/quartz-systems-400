**FREE
//; run: no
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
/define ONE
/if defined(ONE)
dcl-s a char(5) inz('one');
/else
dcl-s a char(5) inz('none');
dcl-s b char(5);
/endif
/if not defined(TWO)
dcl-s c int(10);
/elseif defined(ONE)
dcl-s c int(5);
/else
dcl-s c int(3);
/endif
/undefine ONE
/if defined(ONE)
c = 1;
c = 2;
c = 3;
/endif
line = a + %char(c);
write QSYSPRT line;
*inlr = *on;
