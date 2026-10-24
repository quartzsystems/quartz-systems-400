**FREE
//; dds: B8E
//; keys: 3,10=150 enter
//; keys: reset 3,10=50 enter
//; keys: reset 4,10=PAID enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8E workstn;
// ERRMSG on a field until the input is right.
dou not *in50 and not *in51;
exfmt ENTRY;
if *in03;
leave;
endif;
*in50 = AMT > 99;
*in51 = NOTE = *blanks;
enddo;
DONE = 'Accepted ' + %char(AMT) + ' ' + NOTE;
exfmt ENTRY;
*inlr = *on;
