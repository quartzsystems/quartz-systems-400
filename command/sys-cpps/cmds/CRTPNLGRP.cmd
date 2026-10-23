/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTPNLGRP: create a panel group from UIM source. */
             CMD        PROMPT('Create Panel Group') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHUICMD) HLPID(CRTPNLGRP)
             PARM       KWD(PNLGRP) TYPE(Q1) MIN(1) PROMPT('Panel group')
             PARM       KWD(SRCFILE) TYPE(Q2) DFT(QPNLSRC) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*PNLGRP) +
                          SPCVAL((*PNLGRP)) PROMPT('Source member')
             PARM       KWD(INCFILE) TYPE(Q3) DFT(*SRCFILE) +
                          SNGVAL((*SRCFILE)) PMTCTL(*PMTRQS) +
                          PROMPT('Include file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SRCMBRTXT) +
                          SPCVAL((*SRCMBRTXT) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NOSRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST) MAX(2) +
                          PMTCTL(*PMTRQS) PROMPT('Source listing options')
             PARM       KWD(CHRID) TYPE(E1) DFT(*JOBCCSID) +
                          SNGVAL((*JOBCCSID) (*DEVD)) PMTCTL(*PMTRQS) +
                          PROMPT('Character identifier')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Replace panel group')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*ALL) (*CHANGE) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) RANGE(1 32767) +
                          PROMPT('Graphic character set')
                        ELEM       TYPE(*INT4) RANGE(1 32767) +
                          PROMPT('Code page')
