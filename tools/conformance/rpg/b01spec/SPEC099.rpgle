      *; with: SPC099C
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /COPY QRPGLESRC,SPC099C
     C                   EVAL      LINE = CPYFLD + ' ' + %CHAR(CPYNUM)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
