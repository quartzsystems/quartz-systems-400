     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLONGER_NAME_FO...
     DR_A_DATE_VALUE   S             40    VARYING INZ('a fair-
     D                                     ly long piece of text')
     DA_COUNTER        S              9P 2 INZ(3166.68)
     DCUSTOMER...
     D_NAME_FIELD      S              5S 0 INZ(45)
     DTHE_QUANTITY...
     D_ON_HAND         S              3    DIM(2)
     D                                     INZ('ab')
     C                   EvAl      cusTOMER_naME_fieLD =
     C                               cuStomEr_namE_FIELD * 2 + 1
     C                   eval      LINE = '[' +                                 comment area
     C                              LONGER_NAME_FOR_A_DATE_VALUE
     C                                + '] ' +
     C                                %CHAR(%LEN(LONGER_NAME_FOR_A_DATE_VALUE))
     C                   WRITE     QSYSPRT       LINE
       // a free-form comment
     C                   eval      line = %char(a_counter) + ' ' +              comment area
     C                             %char(customer_name_field) + ' ' +
     C                               %char(%elem(the_quantity_on_hand))
     C                   WRITE     QSYSPRT       LINE

     C                   EVAL      A_COUNTER =                                  comment area
     C                                A_COUNTER +
     C                             CUSTOMER_NAME_FIELD
     C                                / 4
     C                   EVaL      LIne = %cHAr(a_CountEr) + ' ' +
     C                             thE_QUaNtIty_ON_HaND(2)
     c                   wRIte     QSysPRT       liNE
     C                   EVAL      *inlr = *on
