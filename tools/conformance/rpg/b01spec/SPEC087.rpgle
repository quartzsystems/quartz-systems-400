**FREE
// header comment
ctl-opt dftactgrp(*no); // trailing
dcl-f QSYSPRT printer(132); // printer
dcl-ds line len(132) end-ds; // the line
dcl-s a int(10)   // a comment inside a declaration
      inz(5);     // ends here
dcl-s s varchar(20) inz('a // not comment'); // literal holds //
// a comment with a ; semicolon

   // indented comment
a = a     // first operand
    + 2;  // second
line = %char(a) + ' ' + s; // assign
write QSYSPRT line;
*inlr = *on; // done
