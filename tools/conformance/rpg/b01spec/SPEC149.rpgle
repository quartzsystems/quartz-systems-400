     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_AMO...
     DUNT_DUE_TODAY    S             40    VARYING
     D                                     INZ('the quick-
     D                                      brown fox')
     DCUSTOM...
     DER_NAME_FIELD    S              9P 2 INZ(2085.17)
     DTHE_QUANTITY_...
     DON_HAND          S              5S 0 INZ(39)
     DA_COUNTER        S              3    DIM(5)
     D                                     INZ('ab')
       // a free-form comment
     C                   EVAL      THE_QUANTITY_ON_HAND =
     C                              THE_QUANTITY_ON_HAND * 2 + 1
      * a comment line
     C                   EVAL      LINE = '[' + TOTAL_AMOUNT_DUE_TODAY +        comment area
     C                                '] ' +
     C                                %CHAR(%LEN(TOTAL_AMOUNT_DUE_TODAY))
     C                   WrITE     QsySprT       Line
       // a free-form comment
     C                   EVAL      LINE = %CHAR(CUSTOMER_NAME_FIELD) + ' '
     C                              + %CHAR(THE_QUANTITY_ON_HAND) + ' ' +
     C                             %CHAR(%ELEM(A_COUNTER))
     C                   WriTe     qSysPRt       LINe
     C                   eval      CUSTOMER_NAME_FIELD =
     C                                CUSTOMER_NAME_FIELD +
     C                               THE_QUANTITY_ON_HAND / 4
     C                   EVAL      line = %char(customer_name_field) +
     C                               ' ' + a_counter(2)
     C                   WriTE     QSYSPRt       line
     C                   EVAL      *inlr = *on
