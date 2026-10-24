     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DSHORTNM          S             40    VARYING
     D                                     INZ('x')
     DCUST...
     DOMER_NAME_FIELD  S              9P 2 INZ(2143.21)
     DTOTAL_AMOUNT_DUE_...
     DTODAY            S              5S 0 INZ(16)
     DA_COUNTER        S              3    DIM(2)
     D                                     INZ('ab')
     C                   eval      TOTAL_AMOUNT_DUE_TODAY                       comment area
     C                             =
     C                             TOTAL_AMOUNT_DUE_TODAY
     C                             * 2 + 1
     C                   eVAl      Line = '[' +
     C                              SHORTNm + '] ' +
     C                             %char(%leN(shoRTNm))
     c                   wriTe     qsYSpRt       line
     C                   eval      LINE =
     C                              %CHAR(CUSTOMER_NAME_FIELD)
     C                              + ' ' +
     C                             %CHAR(TOTAL_AMOUNT_DUE_TODAY)
     C                             + ' ' +
     C                                %CHAR(%ELEM(A_COUNTER))
     c                   write     qsysprt       line
      * a comment line
     C                   eval      custoMER_NAme_field =
     C                              cuStomeR_NAmE_fIELD +
     C                                TotAl_AMOuNt_DuE_tODAy / 4
     C                   EVAL      line = %char(customer_name_field) + ' ' +
     C                               a_counter(2)
     C                   WRITE     QSYSPRT       LINE
     C                   evAl      *INlr = *On
