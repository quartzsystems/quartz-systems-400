     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_AMOU...
     DNT_DUE_TODAY     S             40    VARYING
     D                                     INZ('keep  two  blanks')
     DTX               S              9P 2
     D                                     INZ(5247.92)
     DCUSTOMER_NAME_FIE...
     DLD               S              5S 0 INZ(33)
     DLONGER_NAME_FOR_A_D...
     DATE_VALUE        S              3    DIM(5)
     D                                     INZ('ab')
     C                   eVAL      customer_name_field
     C                             =
     C                                customer_name_field
     C                                * 2 + 1
     C                   eval      line = '[' +
     C                               total_amount_due_today
     C                             + '] ' +
     C                             %char(%len(total_amount_due_today))
     c                   write     qsysprt       line
     C                   eval      LINE = %CHAR(TX) +
     C                                ' ' +
     C                               %CHAR(CUSTOMER_NAME_FIELD)
     C                              + ' ' +
     C                              %CHAR(%ELEM(LONGER_NAME_FOR_A_DATE_VALUE))
     C                   WRITE     QSYSPRT       LINE
       // a free-form comment
     C                   eval      TX = TX +
     C                             CUSTOMER_NAME_FIELD / 4
     C                   eVAl      line = %char(tx) + ' ' +
     C                             longer_name_for_a_date_value(2)
     C                   wrITE     qsysPRT       Line
     C                   EVAL      *iNlR = *ON
