**FREE
ctl-opt dftactgrp(*no) main(go);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-proc go;
  dcl-s n packed(3:0) inz(999);
  line = 'start';
  write QSYSPRT line;
  n += 1;
  line = 'not reached';
  write QSYSPRT line;
end-proc;
