     HDFTACTGRP(*NO)
      *; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE) OUTPUT(*PRINT)
      *; sqlfile: EMPT
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DNM               S             20A   VARYING
      * WHENEVER NOT FOUND and SQLERROR with GO TO and CONTINUE.
     C/EXEC SQL WHENEVER NOT FOUND GO TO NOTFND
     C/END-EXEC
     C/EXEC SQL SELECT NAME INTO :NM FROM EMPT WHERE ID = 77
     C/END-EXEC
     C                   EVAL      LINE = 'found'
     C                   WRITE     QSYSPRT       LINE
     C     NOTFND        TAG
     C                   EVAL      LINE = 'notfound ' + %CHAR(SQLCOD)
     C                   WRITE     QSYSPRT       LINE
     C/EXEC SQL WHENEVER NOT FOUND CONTINUE
     C/END-EXEC
     C/EXEC SQL SELECT NAME INTO :NM FROM EMPT WHERE ID = 78
     C/END-EXEC
     C                   EVAL      LINE = 'went on ' + %CHAR(SQLCOD)
     C                   WRITE     QSYSPRT       LINE
     C/EXEC SQL WHENEVER SQLERROR GO TO ERR
     C/END-EXEC
     C/EXEC SQL INSERT INTO EMPT (ID, NAME) VALUES (1, 'Dup')
     C/END-EXEC
     C                   EVAL      LINE = 'no error'
     C                   WRITE     QSYSPRT       LINE
     C     ERR           TAG
     C                   EVAL      LINE = 'error ' + %CHAR(SQLCOD)
     C                   WRITE     QSYSPRT       LINE
     C/EXEC SQL WHENEVER SQLERROR CONTINUE
     C/END-EXEC
     C/EXEC SQL INSERT INTO EMPT (ID, NAME) VALUES (1, 'Dup')
     C/END-EXEC
     C                   EVAL      LINE = 'again ' + %CHAR(SQLCOD)
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
