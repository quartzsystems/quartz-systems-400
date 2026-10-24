     HDFTACTGRP(*NO)
      *; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
      *; run: no
      *; sqlfile: EMPT
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DID               S             10I 0
     DNM               S             20A   VARYING
     DN                S             10I 0
     C                   Z-ADD     1             N
     C/EXEC SQL SELECT NAME
     C+   INTO :NM
     C+   FROM EMPT WHERE ID = :ID
     C/END-EXEC
     C                   EVAL      N = U1
     C/EXEC SQL WHENEVER SQLERROR GO TO ERR
     C/END-EXEC
     C/EXEC SQL DECLARE C1 CURSOR FOR SELECT ID FROM EMPT
     C/END-EXEC
     C                   EVAL      N = U2
     C/EXEC SQL OPEN C1
     C/END-EXEC
     C                   EVAL      N = U3
     C/EXEC SQL FETCH C1 INTO :ID
     C/END-EXEC
     C                   EVAL      N = U4
     C/EXEC SQL WHENEVER SQLERROR CONTINUE
     C/END-EXEC
     C/EXEC SQL CLOSE C1
     C/END-EXEC
     C                   EVAL      N = U5
     C     ERR           TAG
     C/EXEC SQL UPDATE EMPT SET SAL = 0 WHERE ID = :ID
     C/END-EXEC
     C                   EVAL      N = U6
     C                   SETON                                        LR
