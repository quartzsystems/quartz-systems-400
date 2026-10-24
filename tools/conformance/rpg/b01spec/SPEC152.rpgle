     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DTHE_QUANTITY_...
     DON_HAND          S             40    VARYING INZ('alpha beta')
     DLONGER_NAME_FOR_A_D...
     DATE_VALUE        S              9P 2 INZ(7368.81)
     DCUSTOMER_NAME...
     D_FIELD           S              5S 0 INZ(5)
     DSHORTN...
     DM                S              3    DIM(4)
     D                                     INZ('ab')
      * a comment line
     C                   EVAL      cUstOMeR_nAMe_field =                        comment area
     C                             cuSTomer_NAMe_FiELd * 2 + 1
     C                   EVAL      LINe = '[' +
     C                                the_QUAnTitY_oN_hAnD
     C                              + '] ' +
     C                             %CHAr(%leN(The_qUANtITY_on_HAnd))
     C                   WRITE     QSYSPRT       LINE
       // a free-form comment
     C                   eval      LinE =                                       comment area
     C                             %chAR(LOnGEr_nAme_For_A_daTe_vAlUE)
     C                               + ' ' +
     C                              %CHar(CUstoMEr_naME_fielD)
     C                                + ' ' +
     C                             %CHaR(%eLEm(ShoRtNm))
     C                   wrITe     qSyspRT       lINE
     C                   EVAl      longer_name_for_a_date_value =
     C                                longer_name_for_a_date_value +
     C                                customer_name_field / 4
     C                   eval      line =
     C                             %char(longer_name_for_a_date_value) +
     C                               ' ' + shortnm(2)
     c                   write     qsysprt       line
     C                   evaL      *inlr = *on
