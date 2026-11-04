**FREE
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name})
//; cmd+: DBGVIEW(*SOURCE) OUTPUT(*PRINT)
ctl-opt dftactgrp(*no);
dcl-s name char(10) inz('QUARTZ');
dcl-s count int(10) inz(42);
dcl-s amt packed(7:2) inz(-123.45);
dcl-s z zoned(5:1) inz(12.3);
dcl-s flag ind inz(*on);
dcl-s v varchar(20) inz('hello');
dcl-s arr int(5) dim(3) inz(7);
dcl-ds ds qualified;
  a char(3) inz('abc');
  b packed(3:0) inz(5);
end-ds;
*in05 = *on;
dump(a);
*inlr = *on;
