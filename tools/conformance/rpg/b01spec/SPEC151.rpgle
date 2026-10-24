     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCUSTOMER_...
     DNAME_FIELD       S             40    VARYING INZ('one, two, th-
     D                                     ree')
     DSHORTNM          S              9P 2 INZ(962.91)
     DTX               S              5S 0 INZ(19)
     DLONGER_NAME_FOR_A_...
     DDATE_VALUE       S              3    DIM(4)
     D                                     INZ('ab')
      * a comment line
     C                   eval      TX = TX * 2 + 1
     C                   eval      LINE = '[' + CUSTOMER_NAME_FIELD +
     C                              '] ' +
     C                                %CHAR(%LEN(CUSTOMER_NAME_FIELD))
     C                   WRITE     QSYSPRT       LINE
     C                   eval      line =
     C                              %char(shortnm)
     C                               + ' ' +
     C                             %char(tx) +
     C                               ' ' +
     C                                %char(%elem(longer_name_for_a_date_value))
     c                   write     qsysprt       line
       // a free-form comment
     C                   eval      SHORTNM = SHORTNM + TX                       comment area
     C                                / 4
     C                   EVAL      LiNe = %cHAr(sHORtNM) + ' ' +                comment area
     C                              LongER_nAME_FOR_A_dAte_valuE(2)
     C                   WRITE     QSYSPRT       LINE
     C                   EVAL      *inLR = *ON
