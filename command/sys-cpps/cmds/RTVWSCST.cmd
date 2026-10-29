/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVWSCST: write the customizing source of a printer model into a */
/* source member, to change and compile with CRTWSCST.               */
             CMD        PROMPT('Retrieve WSCST') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHUCCMD) HLPID(RTVWSCST)
             PARM       KWD(DEVTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*TRANSFORM) +
                          PROMPT('Device type')
             PARM       KWD(MFRTYPMDL) TYPE(*CHAR) LEN(15) +
                          PMTCTL(PT) PROMPT('Manufacturer type and model')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Source member')
             PARM       KWD(SRCFILE) TYPE(Q1) PROMPT('Source file')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text ''description''')
             PT:        PMTCTL     CTL(DEVTYPE) COND((*EQ *TRANSFORM))
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QTXTSRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
