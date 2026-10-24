**FREE
//; dds: B8K
//; keys: 6,2:1 enter
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8K workstn sfile(SFL2: rrn);
dcl-s rrn packed(4: 0);
dcl-s i int(10);
dcl-s n int(10);
// SFLNXTCHG: an updated record is read by READC again.
*in33 = *on;
write CTL2;
*in33 = *off;
for i = 1 to 3;
ITEM = 'KEEP' + %char(i);
DESC = *blanks;
OPT = ' ';
rrn = i;
write SFL2;
endfor;
*in31 = *on;
*in34 = *on;
MSG = 'Mark one';
dou *in03;
write FTR2;
exfmt CTL2;
if *in03;
leave;
endif;
n = 0;
readc SFL2;
dow not %eof;
n += 1;
DESC = 'Seen ' + %char(n) + ' opt ' + OPT;
*in50 = *on;
update SFL2;
*in50 = *off;
readc SFL2;
enddo;
MSG = 'Read ' + %char(n);
enddo;
*inlr = *on;
