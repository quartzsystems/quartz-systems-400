     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_AMOUNT_D...
     DUE_TODAY         S             40    VARYING
     D                                     INZ('the-
     D                                      quick brown fox')
     DTHE_QUANTITY_ON_H...
     DAND              S              9P 2
     D                                     INZ(2261.32)
     DCUSTOM...
     DER_NAME_FIELD    S              5S 0 INZ(17)
     DA_COUNTER        S              3    DIM(3)
     D                                     INZ('ab')
     C                   eval      CUSTOMER_NAME_FIELD
     C                              =
     C                              CUSTOMER_NAME_FIELD
     C                                * 2 + 1
     C                   evAL      lINe = '[' +
     C                                TOtAl_aMouNt_duE_ToDay + '] ' +
     C                              %cHar(%LeN(tOtAL_AmouNT_dUE_toDaY))
     C                   WRITE     QSYSPRT       LINE
      * a comment line
     C                   eval      LINE =                                       comment area
     C                             %CHAR(THE_QUANTITY_ON_HAND)
     C                              + ' ' +
     C                             %CHAR(CUSTOMER_NAME_FIELD)
     C                               + ' ' +
     C                               %CHAR(%ELEM(A_COUNTER))
     c                   write     qsysprt       line
     C                   EVAL      the_qUAnTiTy_ON_HANd =                       comment area
     C                               the_quantIty_oN_HANd +
     C                               CUSTOMEr_NamE_FielD / 4
     C                   EVAL      LINE =
     C                               %CHAR(THE_QUANTITY_ON_HAND)
     C                               + ' ' +
     C                                A_COUNTER(2)
     c                   WriTe     qsysPRt       liNe
     C                   EVAL      *InlR = *On
