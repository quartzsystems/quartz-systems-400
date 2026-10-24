     HDFTACTGRP(*NO)
     FQSYSPRT   O    F  132        PRINTER
     DARR              S             10    DIM(3) CTDATA PERRCD(1)
     D                                     TOFILE(QSYSPRT)
     C                   EVAL      ARR(2) = 'changed'
     C                   EVAL      *INLR = *ON
**CTDATA ARR
first
second
third
