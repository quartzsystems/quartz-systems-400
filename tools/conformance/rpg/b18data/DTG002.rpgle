**FREE
//; with: DTGG
//; setup: CRTBNDRPG PGM({lib}/DTGG) SRCFILE({lib}/QRPGLESRC) SRCMBR(DTGG) DBGVIEW(*NONE)
//; cleanup: DLTPGM PGM({lib}/DTGG)
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds MyRec qualified;
  FirstName char(5) inz('Al');
  Amt packed(7:2) inz(0.5);
  Neg packed(5:3) inz(-0.25);
  Whole packed(5) inz(100);
  Zero zoned(3:1) inz(0);
  TM time inz(t'01.02.03');
  US date(*usa) inz(d'2026-12-31');
  Big uns(10) inz(4000000000);
  num_Kids int(10) inz(1);
  Kids char(1) dim(3) inz('k');
end-ds;
dcl-s out varchar(4000);
data-gen MyRec %data(out) %gen('DTGG');
show('mixed');
data-gen MyRec %data(out : 'countprefix=num_') %gen('DTGG');
show('count');
data-gen MyRec.Kids %data(out) %gen('DTGG');
show('subarr');
*inlr = *on;

dcl-proc show;
  dcl-pi *n;
    what varchar(10) const;
  end-pi;
  dcl-s at int(10);
  dcl-s from int(10) inz(1);
  line = what + ' ' + %char(%len(out));
  write QSYSPRT line;
  at = %scan(x'25' : out);
  dow at > 0;
    line = ' ' + %subst(out : from : at - from);
    write QSYSPRT line;
    from = at + 1;
    if from > %len(out);
      leave;
    endif;
    at = %scan(x'25' : out : from);
  enddo;
  if from <= %len(out);
    line = ' rest ' + %subst(out : from);
    write QSYSPRT line;
  endif;
end-proc;
