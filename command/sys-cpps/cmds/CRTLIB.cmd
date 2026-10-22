/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTLIB: create a library. The keywords, types, defaults and special */
/* values are IBM's; the prompt texts are ours.                       */
             CMD        PROMPT('Create Library') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CRTLIB)
             PARM       KWD(LIB) TYPE(*SNAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*PROD) VALUES(*PROD *TEST) +
                          PROMPT('Library type')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(ASP) TYPE(*INT2) DFT(1) RANGE(1 32) +
                          PMTCTL(*PMTRQS) PROMPT('Auxiliary storage pool')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*ASP) +
                          SPCVAL((*ASP) (*ASPGRPPRI)) PMTCTL(*PMTRQS) +
                          PROMPT('ASP device')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             PARM       KWD(CRTAUT) TYPE(*NAME) LEN(10) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority for new objects')
             PARM       KWD(CRTOBJAUD) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NONE *USRPRF +
                          *CHANGE *ALL) PMTCTL(*PMTRQS) +
                          PROMPT('Auditing for new objects')
