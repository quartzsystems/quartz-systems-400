     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTX               S             40    VARYING INZ('o-
     D                                     ne, two, three')
     DA_COUNTER        S              9P 2
     D                                     INZ(7360.42)
     DSH...
     DORTNM            S              5S 0 INZ(37)
     DTOTAL_AMOUNT_DU...
     DE_TODAY          S              3    DIM(3)
     D                                     INZ('ab')
     C                   EVAL      shoRtnM =
     C                                SHOrTNM * 2 + 1
     C                   EVAL      line = '[' + tx
     C                              + '] ' +
     C                               %char(%len(tx))
     C                   wrItE     QSySpRt       lINe
     C                   eval      LINE =
     C                             %CHAR(A_COUNTER) +
     C                              ' ' + %CHAR(SHORTNM)
     C                                + ' ' +
     C                             %CHAR(%ELEM(TOTAL_AMOUNT_DUE_TODAY))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      a_counter = a_counter +
     C                             shortnm / 4
     C                   eVaL      line = %char(a_counter)                      comment area
     C                                + ' ' +
     C                               total_amount_due_today(2)
     C                   WRiTe     QSysPrT       LiNE
     C                   EVAL      *inlr = *on
