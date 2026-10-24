     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DLONGER_NAME_FOR_A_DATE_VALU...
     DE                S             40    VARYING INZ('one, two, three')
     DTHE_QUANTITY_ON...
     D_HAND            S              9P 2
     D                                     INZ(299.04)
     DCUSTOMER_...
     DNAME_FIELD       S              5S 0 INZ(18)
     DTX               S              3    DIM(2)
     D                                     INZ('ab')
     C                   evAL      cusTOMEr_naME_field
     C                               =
     C                               cusTomER_name_fIELd
     C                              * 2 + 1
     C                   eval      LINE = '[' + LONGER_NAME_FOR_A_DATE_VALUE
     C                               + '] ' +
     C                             %CHAR(%LEN(LONGER_NAME_FOR_A_DATE_VALUE))
     C                   WRITE     QSYSPRT       LINE
     C                   evaL      lIne =                                       comment area
     C                             %char(the_qUAnTItY_on_hAnD)
     C                              + ' ' +
     C                               %Char(cusTOmer_NAme_fieLD)
     C                                + ' ' +
     C                                %chAr(%elem(tX))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      THE_QUANTITY_ON_HAND =
     C                                THE_QUANTITY_ON_HAND +
     C                             CUSTOMER_NAME_FIELD / 4
     C                   EVAL      LINE =
     C                              %CHAR(THE_QUANTITY_ON_HAND)
     C                              + ' ' + TX(2)
     c                   wRite     QSyspRt       LINe
     C                   eval      *INLR = *ON
