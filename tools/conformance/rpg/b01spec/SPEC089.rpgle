00100FQSYSPRT   O    F  132        PRINTER
00200DLINE             DS           132
00300DN                S              5  0 INZ(4)
00400  n = n * 10;                                                              junk; n = 0;
00500  line = 'n=' +                                                            more junk
00600         %char(n);                                                         x
00700C                   WRITE     QSYSPRT       LINE
00800C                   EVAL      *INLR = *ON
