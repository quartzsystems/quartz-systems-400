     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DCUSTOMER_NAME_FI...
     DELD              S             40    VARYING INZ('keep  two -
     D                                      blanks')
     DTX               S              9P 2 INZ(3624.89)
     DSHORTNM          S              5S 0 INZ(9)
     DTHE_QUANTITY_ON...
     D_HAND            S              3    DIM(3)
     D                                     INZ('ab')
      * a comment line
     C                   EvaL      SHORTNM = SHORTNM * 2
     C                                + 1
       // a free-form comment
     C                   Eval      line = '[' +
     C                              customer_name_field +
     C                                '] ' +
     C                               %char(%len(customer_name_field))
     C                   writE     qSysPrT       LIne
     C                   EVAL      LINE = %CHAR(TX)
     C                             + ' ' +
     C                               %CHAR(SHORTNM) +
     C                               ' ' +
     C                             %CHAR(%ELEM(THE_QUANTITY_ON_HAND))
     C                   WrITe     qsYSPrt       Line
     C                   EVAL      tX = tX + shOrTnM /
     C                              4
     C                   EVAL      LiNE = %cHAR(TX) + ' ' +
     C                             THE_qUAntitY_oN_hANd(2)
     c                   write     qsysprt       line
     C                   EVAL      *INLR = *ON
