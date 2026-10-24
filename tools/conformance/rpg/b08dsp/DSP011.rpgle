**FREE
//; dds: B8K
//; keys: 6,2:1 8,2:4 enter
//; keys: 7,2:1 enter
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8K workstn sfile(SFL2: rrn);
dcl-s rrn packed(4: 0);
dcl-s i int(10);
dcl-s n int(10);
// Options typed in a subfile, read back with READC and updated.
*in33 = *on;
write CTL2;
*in33 = *off;
for i = 1 to 5;
ITEM = 'ITEM' + %char(i);
DESC = 'Description ' + %char(i);
OPT = ' ';
rrn = i;
write SFL2;
endfor;
*in31 = *on;
*in34 = *on;
MSG = 'Type options';
dou *in03;
write FTR2;
exfmt CTL2;
if *in03;
leave;
endif;
n = 0;
readc SFL2;
dow not %eof(B8K);
n += 1;
select;
when OPT = '1';
DESC = 'Marked at ' + %char(rrn);
when OPT = '4';
DESC = 'Dropped at ' + %char(rrn);
endsl;
OPT = ' ';
update SFL2;
readc SFL2;
enddo;
MSG = %char(n) + ' changed';
enddo;
*inlr = *on;
