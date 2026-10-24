**FREE
//; dds: B8K
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8K workstn sfile(SFL2: rrn);
dcl-s rrn packed(4: 0);
dcl-s i int(10);
// CHAIN a subfile record by number, change it, UPDATE; %FOUND.
*in33 = *on;
write CTL2;
*in33 = *off;
for i = 1 to 6;
ITEM = 'ROW' + %char(i);
DESC = 'Before';
rrn = i;
write SFL2;
endfor;
chain 3 SFL2;
if %found(B8K);
DESC = 'After chain ' + ITEM;
update SFL2;
endif;
chain 9 SFL2;
MSG = 'found 9 ' + %found(B8K) + ' item ' + ITEM;
rrn = 5;
chain rrn SFL2;
DESC = 'Five';
update SFL2;
*in31 = *on;
*in34 = *on;
dou *in03;
write FTR2;
exfmt CTL2;
enddo;
*inlr = *on;
