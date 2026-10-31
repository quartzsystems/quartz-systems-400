/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDEVOPT: create an optical device description; RSRCNAME(*VRT)  */
/* makes a virtual optical device over an image catalog.            */
             CMD        PROMPT('Create Device Desc (Optical)') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CRTDEVOPT)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Device description')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*VRT) (*NONE)) KEYPARM(*YES) +
                          PROMPT('Resource name')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(10) DFT(*RSRCNAME) +
                          SPCVAL((*RSRCNAME)) KEYPARM(*YES) +
                          PROMPT('Device type')
             PARM       KWD(LCLINTNETA) TYPE(*CHAR) LEN(15) +
                          DFT(*NONE) SPCVAL((*NONE)) +
                          PROMPT('Local internet address')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SYSOPR) +
                          SNGVAL((*SYSOPR)) PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*CHANGE) +
                          SPCVAL((*CHANGE) (*ALL) (*USE) (*EXCLUDE) +
                          (*LIBCRTAUT)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
