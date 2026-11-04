**FREE
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name})
//; cmd+: DBGVIEW(*SOURCE) OUTPUT(*PRINT)
ctl-opt dftactgrp(*no);
dcl-ds fb len(80) inz end-ds;
dcl-s u ucs2(3) inz(%ucs2('ab'));
dcl-s t time inz(t'10.20.30');
dcl-s ts timestamp inz(z'2026-01-02-03.04.05.000006');
dcl-s b int(3) inz(-2);
dcl-s w uns(20) inz(9);
dcl-s h int(20) inz(-12345678901);
dcl-s bn bindec(4:1) inz(-12.5);
dcl-s long char(70) inz('this value is longer than one line of the dump so it wraps');
dcl-s c14 char(14) inz('fourteen chars');
dcl-s c15 char(15) inz('fifteen chars');
dcl-s vr varchar(5) inz('ab');
*in10 = *on;
*inh1 = *on;
dump(a);
*inh1 = *off;
*inlr = *on;
