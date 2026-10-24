     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLONGER_NAME_FOR_A_DA...
     DTE_VALUE         S             40    VARYING
     D                                     INZ('alpha beta')
     DTX               S              9P 2 INZ(6350.29)
     DCUSTOMER_...
     DNAME_FIELD       S              5S 0 INZ(10)
     DSHORTNM          S              3    DIM(3)
     D                                     INZ('ab')
     C                   EVAL      CUSTOMER_NAME_FIELD =
     C                               CUSTOMER_NAME_FIELD * 2 + 1
     C                   EVAL      line = '[' +
     C                             longer_name_for_a_date_value
     C                             + '] ' +
     C                                %char(%len(longer_name_for_a_date_value))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      LiNE = %cHaR(tX) + ' ' +                     comment area
     C                                %cHAr(cuStOmeR_NAME_FielD) + ' ' +
     C                             %chAR(%ElEm(shOrTNM))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      TX = TX +
     C                             CUSTOMER_NAME_FIELD
     C                                / 4
     C                   EVAl      line = %chAr(tX)                             comment area
     C                                + ' ' +
     C                                sHorTNM(2)
     c                   write     qsysprt       line
     C                   EVAL      *inlr = *on
