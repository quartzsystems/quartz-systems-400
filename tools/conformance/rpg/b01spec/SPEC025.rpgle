**FREE
ctl-opt dftactgrp(*no) actgrp(*new) main(entry);
dcl-f QSYSPRT printer(132);
dcl-proc entry;
  dcl-ds line len(132) end-ds;
  dcl-s i int(10);
  for i = 1 to 2;
    line = 'linear main ' + %char(i);
    write QSYSPRT line;
  endfor;
end-proc;
