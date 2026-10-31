/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTIMGCLG: create an image catalog over an IFS directory.         */
             CMD        PROMPT('Create Image Catalog') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(CRTIMGCLG)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(DIR) TYPE(*PNAME) LEN(1024) MIN(1) +
                          CASE(*MIXED) PROMPT('Directory')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*OPT) VALUES(*OPT *TAP) +
                          PROMPT('Image catalog type')
             PARM       KWD(CRTDIR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Create directory')
             PARM       KWD(IMPORT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) +
                          PROMPT('Import image catalog')
             PARM       KWD(CLGASPTHLD) TYPE(*INT2) DFT(*CALC) +
                          RANGE(1 99) SPCVAL((*CALC 0)) +
                          PROMPT('Catalog ASP threshold')
             PARM       KWD(ADDVRTVOL) TYPE(*INT2) DFT(*NONE) +
                          RANGE(1 256) SPCVAL((*NONE 0) (*DIR -1)) +
                          PROMPT('Add virtual volumes')
             PARM       KWD(IMGSIZ) TYPE(*INT4) DFT(*DVD4700) +
                          RANGE(1 16000000) SPCVAL((*CD650 650) +
                          (*DVD2600 2600) (*DVD4700 4700)) PMTCTL(PV) +
                          PROMPT('Image size')
             PARM       KWD(REFIMGCLG) TYPE(*NAME) LEN(10) +
                          PROMPT('Reference image catalog')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Text ''description''')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*EXCLUDE) +
                          SPCVAL((*EXCLUDE) (*CHANGE) (*ALL) (*USE)) +
                          PMTCTL(*PMTRQS) PROMPT('Authority')
             PV:        PMTCTL     CTL(ADDVRTVOL) COND((*GT 0))
