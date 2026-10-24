     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_A...
     DMOUNT_DUE_TODAY  S             40    VARYING
     D                                     INZ('k+
     D                                           eep  two  blanks')
     DTX               S              9P 2
     D                                     INZ(2660.46)
     DCUSTOMER_NAME_FIEL...
     DD                S              5S 0 INZ(28)
     DSHORTNM          S              3    DIM(2)
     D                                     INZ('ab')
     C                   EVal      customer_name_field =
     C                                customer_name_field * 2 + 1
     C                   eval      line = '[' +
     C                              total_amount_due_today
     C                              + '] ' +
     C                               %char(%len(total_amount_due_today))
     c                   write     qsysprt       line
     C                   Eval      line = %char(tx) +
     C                              ' ' +
     C                                %char(customer_name_field)
     C                             + ' ' +
     C                              %char(%elem(shortnm))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      TX = TX + CUSTOMER_NAME_FIELD / 4
     C                   EVAL      LINE = %CHAR(TX) + ' ' +                     comment area
     C                              SHORTNM(2)
     c                   write     qsysprt       line
     C                   EVAL      *inLR = *ON
