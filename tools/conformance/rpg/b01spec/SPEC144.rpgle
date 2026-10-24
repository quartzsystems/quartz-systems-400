     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DSHORTNM          S             40    VARYING
     D                                     INZ('x')
     DTHE_QUANTITY_...
     DON_HAND          S              9P 2
     D                                     INZ(6810.20)
     DTOTAL_AMOUNT_DUE_TODA...
     DY                S              5S 0 INZ(22)
     DCUSTOMER...
     D_NAME_FIELD      S              3    DIM(5)
     D                                     INZ('ab')
      * a comment line
     C                   eval      TOTAL_AMOUNT_DUE_TODAY                       comment area
     C                              =
     C                                TOTAL_AMOUNT_DUE_TODAY
     C                               * 2 + 1
     C                   EVAL      line = '[' + ShORTnm + '] ' +
     C                              %cHar(%Len(sHOrtNM))
     C                   WrIte     qsYsPRT       LINe
     C                   eval      LiNE =
     C                             %cHar(THE_qUAnTiTy_oN_HAND)
     C                              + ' ' +
     C                                %ChAR(ToTal_AmOunt_Due_TOday)
     C                             + ' ' +
     C                               %CHAr(%ELeM(cUSToMEr_nAME_fIELD))
     C                   WRITE     QSYSPRT       LINE
     C                   EvAL      the_quantity_on_hand =
     C                                the_quantity_on_hand +
     C                                total_amount_due_today /
     C                               4
     C                   EVAl      line =
     C                              %char(the_quantity_on_hand) + ' '
     C                              + customer_name_field(2)
     C                   WRITE     QSYSPRT       LINE
     C                   eval      *iNLr = *on
