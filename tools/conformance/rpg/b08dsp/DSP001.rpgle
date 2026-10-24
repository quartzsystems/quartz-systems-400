**FREE
//; dds: B8A
//; keys: 3,10=ALICE 4,10=42 enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8A workstn;
// EXFMT with input and output fields; CA03 ends.
MSG = 'Type a name';
dou *in03;
exfmt SCR1;
if not *in03;
MSG = 'Hello ' + %trim(NAME) + ' ' + %char(CNT * 2);
endif;
enddo;
*inlr = *on;
