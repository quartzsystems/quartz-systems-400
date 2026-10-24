     HDFTACTGRP(*NO)
      *; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
      *; sqlfile: EMPT
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DID               S             10I 0
     DNM               S             20A   VARYING
     DSAL              S              9P 2
      * Fixed form: C/EXEC SQL, C+ lines, the short SQLCA names.
     C/EXEC SQL SELECT NAME, SAL
     C+   INTO :NM, :SAL
     C+   FROM EMPT
     C+   WHERE ID = 3
     C/END-EXEC
     C                   EVAL      LINE = %CHAR(SQLCOD) + ' ' + SQLSTT + ' ' +
     C                             NM
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE = %CHAR(SAL)
     C                   WRITE     QSYSPRT       LINE
     C/EXEC SQL DECLARE C1 CURSOR FOR
     C+   SELECT ID FROM EMPT ORDER BY ID DESC
     C/END-EXEC
     C/EXEC SQL OPEN C1
     C/END-EXEC
     C                   DOU       SQLCOD <> 0
     C/EXEC SQL FETCH C1 INTO :ID
     C/END-EXEC
     C                   IF        SQLCOD = 0
     C                   EVAL      LINE = %CHAR(ID) + ' ' + %CHAR(SQLER3)
     C                   WRITE     QSYSPRT       LINE
     C                   ENDIF
     C                   ENDDO
     C                   EVAL      LINE = %CHAR(SQLCOD) + ' ' + SQLSTT + ' ' +
     C                             SQLWN0
     C                   WRITE     QSYSPRT       LINE
     C/EXEC SQL CLOSE C1
     C/END-EXEC
     C                   SETON                                        LR
