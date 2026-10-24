**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
callp three(1:2:3);
callp three(1);
*inlr = *on;
dcl-proc three;
  dcl-pi *n;
    a int(10) const;
    b int(10) const options(*nopass);
    c int(10) const options(*nopass);
  end-pi;
  line = 'parms ' + %char(%parms) + ' b ' + %char(%parmnum(b)) + ' c ' + %char(%parmnum(c));
  if %parms >= %parmnum(c);
    line = %trimr(line) + ' sum ' + %char(a + b + c);
  endif;
  write QSYSPRT line;
end-proc;
