      * Nested conditions; DEFINE names some.
      *; run: no
      *; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT) OPTION(*SHOWSKP) DEFINE(ALPHA)
     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
      /IF DEFINED(ALPHA)
      /IF DEFINED(BETA)
     DX                S             10    INZ('ab')
      /ELSE
     DX                S             10    INZ('a')
      /ENDIF
      /ELSE
     DX                S             10    INZ('none')
      /ENDIF
     C                   EVAL      LINE = X
     C                   WRITE     QSYSPRT       LINE
     C                   SETON                                        LR
