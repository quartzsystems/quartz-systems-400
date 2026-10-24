**FREE
//; dds: B8T
//; keys: 3,10=FIRST enter
//; keys: 3,10=SECOND enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8T workstn;
// WRITE then READ by format, then READ by file name.
SEEN = 'Nothing seen';
write ASK;
read ASK;
SEEN = 'By format: ' + WORD + ' in03 ' + *in03;
write ASK;
read B8T;
SEEN = 'By file: ' + WORD + ' in03 ' + *in03;
dou *in03;
exfmt ASK;
enddo;
*inlr = *on;
