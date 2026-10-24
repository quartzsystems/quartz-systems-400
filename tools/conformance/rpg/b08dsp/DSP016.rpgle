**FREE
//; dds: B8D
//; keys: f3
ctl-opt dftactgrp(*no);
dcl-f B8D workstn usropn;
dcl-s s1 char(5);
dcl-s s2 char(5);
dcl-s o1 ind;
dcl-s o2 ind;
// USROPN: OPEN, OPEN again (01215), %OPEN, CLOSE.
o1 = %open(B8D);
open B8D;
open(e) B8D;
s1 = %editc(%status(B8D): 'X');
o2 = %open(B8D);
RES = 'open before ' + o1 + ' after ' + o2 + ' status ' + s1 + ' err ' + %error;
exfmt KEYS;
close B8D;
s2 = %editc(%status(B8D): 'X');
open B8D;
*inlr = *on;
