/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTSCHIDX: create an empty search index for help. */
             CMD        PROMPT('Create Search Index') MAXPOS(3) +
                          ALLOW(*ALL) +
                          HLPPNLGRP(QHUICMD) HLPID(CRTSCHIDX)
             PARM       KWD(SCHIDX) TYPE(Q1) MIN(1) PROMPT('Search index')
             PARM       KWD(TITLE) TYPE(*CHAR) LEN(55) +
                          PROMPT('Display title')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*TITLE) +
                          SPCVAL((*TITLE) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(CHRID) TYPE(E1) DFT(*SYSVAL) +
                          SNGVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Character identifier')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT4) PROMPT('Graphic character set')
                        ELEM       TYPE(*INT4) PROMPT('Code page')
