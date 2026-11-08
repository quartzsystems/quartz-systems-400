**FREE
ctl-opt dftactgrp(*no) main(MAIN);
dcl-f QSYSPRT printer(132);
// Unnamed INZ subfields under an array laid over them with POS(1), in
// a procedure's automatic storage; CLEAR and RESET of the structure.
dcl-proc MAIN;
  dcl-ds line len(132) end-ds;
  dcl-ds sortTable;
    *n char(12) inz('Name');
    *n char(12) inz('Type');
    *n char(12) inz('Status');
    *n char(12) inz('Manufacturer');
    *n char(12) inz('Model');
    *n char(12) inz('Quantity');
    sortNames char(12) dim(6) pos(1);
  end-ds;
  dcl-s sortKey int(10);
  line = %char(%elem(sortNames));
  write QSYSPRT line;
  for sortKey = 1 to %elem(sortNames);
    line = %char(sortKey) + ' ' + %trim(sortNames(sortKey));
    write QSYSPRT line;
  endfor;
  clear sortTable;
  line = '[' + sortTable + ']';
  write QSYSPRT line;
  sortNames(2) = 'Changed';
  reset sortTable;
  line = sortTable;
  write QSYSPRT line;
end-proc;
