**FREE
//; dds: CUSTF
//; sqlfile: CUSTF
//; dds: B8S
//; keys: pgdn
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f CUSTF keyed;
dcl-f B8S workstn sfile(CSFL: rrn);
dcl-s rrn packed(4: 0);
// Customers read from the database into a subfile.
*in33 = *on;
write CCTL;
*in33 = *off;
read CUSTF;
dow not %eof(CUSTF);
rrn += 1;
write CSFL;
read CUSTF;
enddo;
TOTAL = rrn;
*in31 = rrn > 0;
*in34 = *on;
dou *in03;
exfmt CCTL;
enddo;
*inlr = *on;
