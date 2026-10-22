/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPAUDJRNE: show audit journal entries of the types chosen. */
             CMD        PROMPT('Display Audit Journal Entries') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DSPAUDJRNE)
             PARM       KWD(ENTTYP) TYPE(*CHAR) LEN(2) RSTD(*YES) +
                          MIN(1) MAX(80) VALUES(AD AF AP AU AX CA CD CO +
                          CP CQ CU CV CY DI DO DS EV GR GS IM IP IR IS +
                          JD JS KF LD ML NA ND NE OM OR OW O1 O2 O3 PA +
                          PF PG PO PS PW RA RJ RO RP RQ RU RZ SD SE SF +
                          SG SK SM SO ST SV VA VC VF VL VN VO VP VR VS +
                          VU VV X0 X1 X2 YC YR ZC ZR) +
                          PROMPT('Entry types')
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(JRNRCV) TYPE(Q1) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*CURCHAIN)) +
                          PROMPT('Journal receiver')
             PARM       KWD(FROMTIME) TYPE(E1) PROMPT('Start time')
             PARM       KWD(TOTIME) TYPE(E2) PROMPT('End time')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DATE) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Start date')
                        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Start time')
             E2:        ELEM       TYPE(*DATE) DFT(*LAST) +
                          SPCVAL((*LAST)) PROMPT('End date')
                        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('End time')
