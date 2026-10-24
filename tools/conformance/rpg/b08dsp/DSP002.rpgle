**FREE
//; dds: B8B
//; keys: 5,8=AB12 6,8=7 enter
//; keys: f5
//; keys: 5,8=ZZ enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8B workstn;
dcl-s n int(10);
// A header, a footer and a query record over them (OVERLAY).
HTXT = 'Header written once';
write HDR;
RESULT = 'Nothing yet';
dou *in03;
write FTR;
exfmt QRY;
n += 1;
if *in05;
RESULT = 'Refresh ' + %char(n);
elseif not *in03;
RESULT = 'Code ' + CODE + ' qty ' + %char(QTY) + ' n ' + %char(n);
endif;
enddo;
*inlr = *on;
