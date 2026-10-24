     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DS...
     DHORTNM           S             40    VARYING
     D                                     INZ('alpha b+
     D                                     eta')
     DTOTAL_AMOUNT_DUE_TODA...
     DY                S              9P 2 INZ(3441.62)
     DA...
     D_COUNTER         S              5S 0 INZ(40)
     DCUSTOME...
     DR_NAME_FIELD     S              3    DIM(5)
     D                                     INZ('ab')
     C                   eval      A_coUnTEr = a_cOUnTEr * 2 + 1
     C                   eVAL      line = '[' + shortnm + '] ' +
     C                               %char(%len(shortnm))
     c                   write     qsysprt       line
     C                   eval      LINE =
     C                                %CHAR(TOTAL_AMOUNT_DUE_TODAY)
     C                               + ' ' +
     C                                %CHAR(A_COUNTER)
     C                             + ' ' +
     C                               %CHAR(%ELEM(CUSTOMER_NAME_FIELD))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      total_amount_due_today =
     C                             total_amount_due_today +
     C                              a_counter / 4
     C                   EVAL      line = %char(total_amount_due_today)
     C                                + ' ' + customer_name_field(2)
     c                   writE     QsYsPRT       LiNe
     C                   eval      *inlr = *on
