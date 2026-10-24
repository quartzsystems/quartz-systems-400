     FQSYSPRT   O    F  132        PRINTER
     DLINE             DS           132
     DA_COUNTER        S             40    VARYING
     D                                     INZ('a fairly lon-
     D                                     g piece of text')
     DSHORTNM          S              9P 2
     D                                     INZ(748.14)
     DTHE_QUA...
     DNTITY_ON_HAND    S              5S 0 INZ(6)
     DCUSTOMER_NAME_F...
     DIELD             S              3    DIM(4)
     D                                     INZ('ab')
     C                   EVAl      the_quantity_on_hand =                       comment area
     C                             the_quantity_on_hand * 2 + 1
     C                   EVAL      line = '[' +                                 comment area
     C                              a_counter + '] ' +
     C                               %char(%len(a_counter))
     c                   write     qsysprt       line
     C                   EVAL      line = %char(shortnm) +
     C                             ' ' +
     C                              %char(the_quantity_on_hand)
     C                              + ' ' +
     C                             %char(%elem(customer_name_field))
     C                   WRite     qSysPRt       Line
     C                   EVAL      SHORTNM = SHORTNM +
     C                              THE_QUANTITY_ON_HAND / 4
     C                   EVAL      line = %char(shortnm) + ' ' +
     C                             customer_name_field(2)
     C                   WRITE     QSYSPRT       LINE
     C                   eval      *INLR = *ON
