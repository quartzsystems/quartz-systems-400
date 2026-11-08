**FREE
//; dds: B8U
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8U workstn;
dcl-s after varchar(60);
dcl-s afterall varchar(60);
// CLEAR of a record format: the fields the program sends; *ALL every one.
INP = 'aaaaa';
OUTF = 'bbbbb';
BTH = 'ccccc';
HID = 'ddddd';
NUM = 7;
clear CLR;
after = '[' + INP + '][' + OUTF + '][' + BTH + '][' + HID + '][' + %char(NUM) + ']';
INP = 'aaaaa';
OUTF = 'bbbbb';
BTH = 'ccccc';
HID = 'ddddd';
NUM = 7;
clear *all CLR;
afterall = '[' + INP + '][' + OUTF + '][' + BTH + '][' + HID + '][' + %char(NUM) + ']';
SUM = after;
SUM2 = afterall;
exfmt CLR;
*inlr = *on;
