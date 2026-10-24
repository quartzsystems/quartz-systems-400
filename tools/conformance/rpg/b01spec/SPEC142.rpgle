     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCUSTOMER_NA...
     DME_FIELD         S             40    VARYING
     D                                     INZ('a fair+
     D                                       ly long piece of text')
     DS...
     DHORTNM           S              9P 2
     D                                     INZ(3957.57)
     DLONGER_NAME_FOR_A_DATE_VA...
     DLUE              S              5S 0 INZ(17)
     DTOTAL_AM...
     DOUNT_DUE_TODAY   S              3    DIM(3)
     D                                     INZ('ab')
       // a free-form comment
     C                   eval      LONGER_NAME_FOR_A_DATE_VALUE =
     C                                LONGER_NAME_FOR_A_DATE_VALUE * 2 + 1
      * a comment line
     C                   EVaL      lINe = '[' +
     C                                CuStoMEr_NaMe_FielD
     C                             + '] ' +
     C                               %cHAr(%LeN(cUStOMER_NamE_FieLd))
     c                   WRitE     qsYsprT       lInE
      * a comment line
     C                   eval      LINE = %CHAR(SHORTNM) + ' '
     C                               +
     C                             %CHAR(LONGER_NAME_FOR_A_DATE_VALUE)
     C                                + ' ' +
     C                              %CHAR(%ELEM(TOTAL_AMOUNT_DUE_TODAY))
     c                   wRIte     qSYsPRt       lInE
     C                   eval      SHOrtNM = shoRTNM +
     C                                lOngeR_NAmE_FOR_a_dATe_VAluE
     C                              / 4
     C                   EVAL      line =                                       comment area
     C                              %char(shortnm) +
     C                             ' ' +
     C                               total_amount_due_today(2)
     C                   WRITE     QSYSPRT       LINE
     C                   eval      *inLR = *on
