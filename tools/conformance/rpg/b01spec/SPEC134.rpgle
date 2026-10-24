     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA_COUNTER        S             40    VARYING INZ('alpha be+
     D                                        ta')
     DTX               S              9P 2
     D                                     INZ(6192.38)
     DCUSTOMER_NA...
     DME_FIELD         S              5S 0 INZ(46)
     DLONGER_NAME_FOR_A_DAT...
     DE_VALUE          S              3    DIM(4)
     D                                     INZ('ab')
     C                   EVAL      CUSTOMER_NAME_FIELD =
     C                              CUSTOMER_NAME_FIELD * 2 + 1
       // a free-form comment
     C                   evAl      lINE = '[' + a_CoUNTer + '] ' +
     C                              %ChAr(%LEn(a_coUnTER))
     C                   WRITE     QSYSPRT       LINE
     C                   EvaL      LINE = %CHAR(TX) +
     C                               ' ' +
     C                              %CHAR(CUSTOMER_NAME_FIELD)
     C                                + ' ' +
     C                               %CHAR(%ELEM(LONGER_NAME_FOR_A_DATE_VALUE))
     c                   write     qsysprt       line
     C                   EVAL      tx = tx +
     C                             customer_name_field / 4
     C                   eval      line =                                       comment area
     C                               %char(tx) +
     C                                ' ' +
     C                              longer_name_for_a_date_value(2)
     C                   wriTE     QsySpRT       line
     C                   eval      *INLR = *ON
