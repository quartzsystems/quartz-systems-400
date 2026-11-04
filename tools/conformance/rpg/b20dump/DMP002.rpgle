**FREE
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name})
//; cmd+: DBGVIEW(*SOURCE) OUTPUT(*PRINT)
ctl-opt dftactgrp(*no);
dcl-s x char(5) inz('first');
dcl-s p pointer;
dcl-s d date inz(d'2026-01-15');
dcl-s f float(8) inz(1.5);
dcl-s g float(8) inz(-0.03125);
dcl-s arr char(2) dim(5) inz('ab');
arr(2) = 'cd';
arr(3) = 'cd';
x = 'later';
proc1(7);
*inlr = *on;
dcl-proc proc1;
  dcl-pi *n;
    n int(10) value;
  end-pi;
  dcl-s loc char(4) inz('locl');
  dcl-s m packed(9:3) inz(-1.5);
  dump(a) 'my identifier';
end-proc;
