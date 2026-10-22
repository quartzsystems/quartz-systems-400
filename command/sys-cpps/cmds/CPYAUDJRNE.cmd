/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYAUDJRNE: copy audit journal entries into one output file per entry */
/* type.                                                                  */
             CMD        PROMPT('Copy Audit Journal Entries') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CPYAUDJRNE)
             PARM       KWD(ENTTYP) TYPE(*CHAR) LEN(2) RSTD(*YES) +
                          MIN(1) MAX(80) SNGVAL((*ALL)) VALUES(AD AF AP +
                          AU AX CA CD CO CP CQ CU CV CY DI DO DS EV GR +
                          GS IM IP IR IS JD JS KF LD ML NA ND NE OM OR +
                          OW O1 O2 O3 PA PF PG PO PS PW RA RJ RO RP RQ +
                          RU RZ SD SE SF SG SK SM SO ST SV VA VC VF VL +
                          VN VO VP VR VS VU VV X0 X1 X2 YC YR ZC ZR) +
                          PROMPT('Entry types')
             PARM       KWD(OUTFILE) TYPE(Q1) FILE(*OUT) +
                          PROMPT('Output file prefix')
             PARM       KWD(OUTMBR) TYPE(E1) PROMPT('Output member options')
             PARM       KWD(JRNRCV) TYPE(E2) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*CURCHAIN)) +
                          PROMPT('Journal receivers')
             PARM       KWD(FROMTIME) TYPE(E3) PROMPT('Start time')
             PARM       KWD(TOTIME) TYPE(E4) PROMPT('End time')
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QAUDIT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(QGPL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             E2:        ELEM       TYPE(Q2) PROMPT('Starting receiver')
                        ELEM       TYPE(Q2) PROMPT('Ending receiver')
             E3:        ELEM       TYPE(*DATE) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Start date')
                        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('Start time')
             E4:        ELEM       TYPE(*DATE) DFT(*LAST) +
                          SPCVAL((*LAST)) PROMPT('End date')
                        ELEM       TYPE(*TIME) DFT(*AVAIL) +
                          SPCVAL((*AVAIL)) PROMPT('End time')
