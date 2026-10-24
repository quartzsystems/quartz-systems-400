**FREE
//; dds: B8Q
//; keys: 8,20:WXYZ enter
//; keys: f12
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8Q workstn;
// A window record over a background record.
BTXT = 'Behind the window';
dou *in03;
write BACK;
exfmt WIN1;
if *in12;
BTXT = 'Window cancelled';
else;
BTXT = 'Code ' + WCODE;
endif;
write BACK;
exfmt WIN1;
leave;
enddo;
*inlr = *on;
