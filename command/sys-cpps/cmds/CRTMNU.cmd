/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTMNU: create a menu object over a panel group, a display file */
/* or a program.                                                      */
             CMD        PROMPT('Create Menu') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHUICMD) HLPID(CRTMNU)
             PARM       KWD(MENU) TYPE(Q1) MIN(1) PROMPT('Menu')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*UIM) VALUES(*UIM *DSPF *PGM) +
                          PROMPT('Menu type')
             PARM       KWD(PNLGRP) TYPE(Q2) DFT(*MENU) SNGVAL((*MENU)) +
                          PMTCTL(P1) PROMPT('Panel group')
             PARM       KWD(DSPF) TYPE(Q2) DFT(*MENU) SNGVAL((*MENU)) +
                          PMTCTL(P2) PROMPT('Display file')
             PARM       KWD(MSGF) TYPE(Q2) DFT(*MENU) SNGVAL((*MENU)) +
                          PMTCTL(P2) PROMPT('Message file')
             PARM       KWD(PGM) TYPE(Q2) DFT(*MENU) SNGVAL((*MENU)) +
                          PMTCTL(P3) PROMPT('Program')
             PARM       KWD(CMDLIN) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*LONG) VALUES(*LONG *SHORT *NONE) +
                          PMTCTL(P2) PROMPT('Command line')
             PARM       KWD(CHKLIBL) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*LIBL) VALUES(*LIBL *NOCHK) PMTCTL(P2) +
                          PROMPT('Check library list')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*NOCHG) +
                          SPCVAL((*NOCHG) (*MNULIB) (*CRTDFT)) +
                          PMTCTL(*PMTRQS) PROMPT('Current library')
             PARM       KWD(PRDLIB) TYPE(*NAME) LEN(10) DFT(*NOCHG) +
                          SPCVAL((*NOCHG) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Product library')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text ''description''')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace menu')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*ALL) (*CHANGE) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             P1:        PMTCTL     CTL(TYPE) COND((*EQ *UIM))
             P2:        PMTCTL     CTL(TYPE) COND((*EQ *DSPF))
             P3:        PMTCTL     CTL(TYPE) COND((*EQ *PGM))
