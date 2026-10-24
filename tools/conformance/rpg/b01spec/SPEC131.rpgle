     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTX               S             40    VARYING INZ('one, two, +
     D                                        three')
     DTHE_QUANTIT...
     DY_ON_HAND        S              9P 2 INZ(1700.58)
     DTOTAL_AMOUNT_DUE_...
     DTODAY            S              5S 0 INZ(46)
     DSHORTNM          S              3    DIM(2)
     D                                     INZ('ab')
      * a comment line
     C                   EVal      total_aMOUNT_duE_toDay =
     C                               TOTal_aMouNT_dUe_tOdAY * 2
     C                                + 1
      * a comment line
     C                   evAl      LINE = '[' +
     C                              TX + '] ' +
     C                               %CHAR(%LEN(TX))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      LINE =
     C                              %CHAR(THE_QUANTITY_ON_HAND)
     C                              + ' ' +
     C                             %CHAR(TOTAL_AMOUNT_DUE_TODAY)
     C                             + ' ' +
     C                              %CHAR(%ELEM(SHORTNM))
     c                   write     qsysprt       line
     C                   eval      the_quantity_on_hand
     C                               =
     C                               the_quantity_on_hand
     C                                +
     C                                total_amount_due_today
     C                               / 4
     C                   EVal      line =
     C                                %char(the_quantity_on_hand)
     C                               + ' ' + shortnm(2)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAl      *Inlr = *ON
