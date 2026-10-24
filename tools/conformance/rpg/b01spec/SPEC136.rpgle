     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLONGER_NAME_FO...
     DR_A_DATE_VALUE   S             40    VARYING
     D                                     INZ('x')
     DTX               S              9P 2 INZ(9287.02)
     DTHE_QUANTITY_O...
     DN_HAND           S              5S 0 INZ(21)
     DTOTAL_AMOUNT_DUE_TODA...
     DY                S              3    DIM(4)
     D                                     INZ('ab')
       // a free-form comment
     C                   EVal      tHE_QuaNtITy_ON_HAnD
     C                                =
     C                               tHE_qUanTiTY_ON_hAnD
     C                             * 2 + 1
     C                   eval      LINE = '[' +
     C                              LONGER_NAME_FOR_A_DATE_VALUE +
     C                                '] ' +
     C                               %CHAR(%LEN(LONGER_NAME_FOR_A_DATE_VALUE))
     c                   write     qsysprt       line
      * a comment line
     C                   eVAL      line = %char(tx) + ' ' +                     comment area
     C                              %char(the_quantity_on_hand) +
     C                             ' ' +
     C                             %char(%elem(total_amount_due_today))
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      tx = tx +
     C                              the_quantity_on_hand
     C                              / 4
     C                   EvAL      LINE = %CHAR(TX)                             comment area
     C                                + ' ' +
     C                                TOTAL_AMOUNT_DUE_TODAY(2)
     C                   WRITE     QSYSPRT       LINE
     C                   EVaL      *INLR = *ON
