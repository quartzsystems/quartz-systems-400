     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTOTAL_AM...
     DOUNT_DUE_TODAY   S             40    VARYING
     D                                     INZ('o+
     D                                          ne, two, three')
     DA_COUNTER        S              9P 2
     D                                     INZ(3174.41)
     DLONGER_NAME_FOR_A_DATE_VALU...
     DE                S              5S 0 INZ(23)
     DTHE_QUANTITY_ON_HAN...
     DD                S              3    DIM(3)
     D                                     INZ('ab')
      * a comment line
     C                   eVaL      LONGER_NAME_FOR_A_DATE_VALUE =
     C                                LONGER_NAME_FOR_A_DATE_VALUE * 2 + 1
     C                   eval      lInE = '[' + TOTAl_AmoUNt_dUe_TODAY
     C                                + '] ' +
     C                               %cHar(%LEN(ToTAl_aMoUNT_dUE_TODAY))
     c                   wRIte     Qsysprt       Line
     C                   EVAL      lINE = %Char(A_cOUnTeR) +                    comment area
     C                             ' ' +
     C                             %chAr(LoNGEr_nAmE_For_A_DAtE_VALue)
     C                              + ' ' +
     C                             %CHAR(%eLEM(The_QuAntIty_oN_HaND))
     C                   WrItE     qSysprt       lINe
       // a free-form comment
     C                   evAL      A_COUNTER = A_COUNTER +                      comment area
     C                              LONGER_NAME_FOR_A_DATE_VALUE / 4
     C                   eval      lInE = %chAr(A_COUnteR) + ' ' +
     C                             thE_quANtity_oN_HAnD(2)
     C                   WRite     QsySPRT       lInE
     C                   EVAL      *InlR = *on
