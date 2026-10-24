     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTHIS_IS_A_VERY_LONG_...
     DNAME_SPLIT_OVER_...
     DTHREE_LINES      S              5  0 INZ(3)
     DANOTHER_NAME_THAT_IS_LONG...
     D                 S             10    INZ('second')
       line = %char(THIS_IS_A_VERY_LONG_NAME_SPLIT_OVER_THREE_LINES);
     C                   WRITE     QSYSPRT       LINE
       line = ANOTHER_NAME_THAT_IS_LONG;
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *INLR = *ON
