**FREE
//; dds: B8D
//; keys: 3,10=ABC f5
//; keys: 3,10=XYZ f12
//; keys: 3,10=QQ f10
//; keys: enter
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8D workstn;
// CF keys bring the data back, CA keys do not; *INKx follow the key.
RES = 'Start';
dou *in03;
exfmt KEYS;
RES = 'k05=' + *in05 + ' k10=' + *in10 + ' k12=' + *in12 + ' val=' + VAL;
RES2 = 'ke=' + *inke + ' kj=' + *inkj + ' kl=' + *inkl + ' kc=' + *inkc;
enddo;
*inlr = *on;
