     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA_COU...
     DNTER             S             40    VARYING INZ('alpha +
     D                                          beta')
     DTOTAL_AMOUNT_DUE...
     D_TODAY           S              9P 2
     D                                     INZ(1776.83)
     DLONGER_NAME_FO...
     DR_A_DATE_VALUE   S              5S 0 INZ(35)
     DTX               S              3    DIM(2)
     D                                     INZ('ab')
     C                   EVAL      longer_name_for_a_date_value =
     C                               longer_name_for_a_date_value * 2 + 1
       // a free-form comment
     C                   EVAL      LINE = '[' + A_COUNTER + '] '                comment area
     C                                + %CHAR(%LEN(A_COUNTER))
     C                   WRITE     QSYSPRT       LINE
      * a comment line
     C                   eval      LiNE =
     C                                %char(TOTAl_AMOUnT_duE_todAy)
     C                                + ' ' +
     C                               %CHar(lONgER_nAmE_FOR_a_DAte_vAlue)
     C                               + ' ' + %CHaR(%eLeM(tX))
     c                   WRitE     QSysprt       liNE
      * a comment line
     C                   eval      TOTAL_AMOUNT_DUE_TODAY
     C                               =
     C                                TOTAL_AMOUNT_DUE_TODAY
     C                                +
     C                                LONGER_NAME_FOR_A_DATE_VALUE
     C                              / 4
     C                   eval      line = %char(total_amount_due_today)
     C                             + ' ' + tx(2)
     C                   wriTE     qSYsPrt       LINe
     C                   eVaL      *INLR = *ON
