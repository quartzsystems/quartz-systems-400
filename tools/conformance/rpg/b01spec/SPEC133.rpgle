     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA_COUNTER        S             40    VARYING INZ('x')
     DTHE_QUANTITY_O...
     DN_HAND           S              9P 2 INZ(3142.07)
     DTX               S              5S 0 INZ(25)
     DSHO...
     DRTNM             S              3    DIM(2)
     D                                     INZ('ab')
     C                   EvaL      Tx = tx * 2 + 1                              comment area
       // a free-form comment
     C                   eval      LINE = '[' +
     C                               A_COUNTER + '] '
     C                               +
     C                             %CHAR(%LEN(A_COUNTER))
     c                   wriTe     QSysPrT       lINe
     C                   EVAL      LINe =                                       comment area
     C                              %chaR(The_QuANTity_oN_HAnd)
     C                                + ' ' + %CHAR(Tx)
     C                               + ' ' +
     C                                %chaR(%eleM(shoRtNM))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      THE_QUANTITY_ON_HAND =
     C                                THE_QUANTITY_ON_HAND + TX / 4
     C                   EVAL      line =
     C                               %char(the_quantity_on_hand) +
     C                               ' ' + shortnm(2)
     c                   Write     QSYsPrt       LINE
     C                   eval      *inLr = *on
