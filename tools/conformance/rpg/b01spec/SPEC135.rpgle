     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_AMOUNT_...
     DDUE_TODAY        S             40    VARYING
     D                                     INZ('alpha-
     D                                      beta')
     DCUSTOMER_N...
     DAME_FIELD        S              9P 2 INZ(3688.92)
     DA_COUNTER        S              5S 0 INZ(26)
     DLONGER_NAME_FO...
     DR_A_DATE_VALUE   S              3    DIM(3)
     D                                     INZ('ab')

     C                   EVAL      A_COUNTER =                                  comment area
     C                               A_COUNTER * 2 + 1
     C                   EVaL      line = '[' + total_amount_due_today          comment area
     C                              + '] ' +
     C                               %char(%len(total_amount_due_today))
     c                   write     qsysprt       line
       // a free-form comment
     C                   EvAl      lINE = %cHAR(CUstOmer_namE_FiElD) + ' ' +    comment area
     C                               %ChAr(a_coUntEr) + ' ' +
     C                               %chAR(%EleM(LonGeR_naME_fOR_a_Date_vAlUE))
     c                   write     qsysprt       line
       // a free-form comment
     C                   EVAL      cUStOmEr_nAmE_fiElD =
     C                             CUStoMEr_NaME_FIElD +
     C                                A_coUNTER / 4
     C                   EVAL      LINE =
     C                                %CHAR(CUSTOMER_NAME_FIELD)
     C                               + ' ' +
     C                             LONGER_NAME_FOR_A_DATE_VALUE(2)
     C                   WRITE     QSYSPRT       LINE
     C                   eval      *inlr = *on
