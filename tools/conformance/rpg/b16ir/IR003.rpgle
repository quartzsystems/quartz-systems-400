     H DFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     D line            DS           132
     D v               S               A   LEN(80000) VARYING
     D s               S             10A   VARYING(4)
     D u               S               C   LEN(70000) VARYING
     D t               S             12A   VARYING(2)
     D i               S             10I 0
      /free
        line = 'a ' + %char(%size(v)) + ' ' + %char(%size(s)) + ' ' +
               %char(%size(u)) + ' ' + %char(%size(t));
        write QSYSPRT line;
        for i = 1 to 7999;
          v = v + '0123456789';
        endfor;
        v = v + 'END';
        line = 'b ' + %char(%len(v)) + ' [' + %subst(v:%len(v)-5) + ']';
        write QSYSPRT line;
        s = %subst(v:79988);
        t = %trim('  ' + s + '  ');
        line = 'c ' + %char(%len(s)) + ' [' + s + '] ' + %char(%len(t));
        write QSYSPRT line;
        u = %ucs2(%subst(v:1:30000));
        u = u + u + u;
        line = 'd ' + %char(%len(u)) + ' [' +
               %char(%subst(u:69995:6)) + ']';
        write QSYSPRT line;
        v = *blanks;
        line = 'e ' + %char(%len(v));
        write QSYSPRT line;
        *inlr = *on;
      /end-free
