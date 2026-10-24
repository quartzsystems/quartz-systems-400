**FREE
//; dds: B8L
//; keys: pgdn
//; keys: pgdn
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8L workstn sfile(SFL3: rrn);
dcl-s rrn packed(4: 0);
dcl-s nxt int(10) inz(1);
dcl-s i int(10);
// A page at a time: ROLLUP comes to the program, which loads the next.
exsr load;
dou *in03;
*in31 = rrn > 0;
exfmt CTL3;
if *in25;
exsr load;
endif;
enddo;
*inlr = *on;
begsr load;
*in33 = *on;
write CTL3;
*in33 = *off;
rrn = 0;
for i = 1 to 4;
LNUM = nxt;
LTXT = 'Line ' + %char(nxt);
nxt += 1;
rrn += 1;
write SFL3;
endfor;
PAGES += 1;
*in34 = PAGES >= 3;
endsr;
