**FREE
//; dds: USATF
//; sqlfile: USATF
// What a program sees of a *USA time field: the record buffer of a
// program-described read, and the field of an externally described one.
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-f RAWF disk(20) usage(*input) extfile('USATF') usropn;
dcl-ds buf len(20) end-ds;
dcl-f USATF usage(*input) block(*no);
dcl-ds r likerec(USATR);
dcl-c HEX '0123456789ABCDEF';
open RAWF;
read RAWF buf;
dow not %eof(RAWF);
  line = 'raw [' + buf + '] ' + hexof(buf);
  write QSYSPRT line;
  read RAWF buf;
enddo;
close RAWF;
read USATF r;
dow not %eof(USATF);
  line = 'ext ' + r.K1 + ' ' + %char(r.T1) + ' ' + %char(r.T2) + ' '
    + %char(r.T1 : *ISO) + ' ' + %char(%subdt(r.T1 : *seconds));
  write QSYSPRT line;
  read USATF r;
enddo;
*inlr = *on;

dcl-proc hexof;
  dcl-pi *n varchar(60);
    s char(20) const;
  end-pi;
  dcl-s o varchar(60);
  dcl-s k int(10);
  dcl-s b uns(3);
  dcl-ds c;
    ch char(1);
    n uns(3) pos(1);
  end-ds;
  for k = 1 to 20;
    ch = %subst(s : k : 1);
    b = n;
    o += %subst(HEX : %div(b : 16) + 1 : 1) + %subst(HEX : %rem(b : 16) + 1 : 1);
  endfor;
  return o;
end-proc;
