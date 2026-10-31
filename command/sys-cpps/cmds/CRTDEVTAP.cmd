/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDEVTAP: create a tape device description; RSRCNAME(*VRT) makes */
/* a virtual tape device over an image catalog.                      */
             CMD        PROMPT('Create Device Desc (Tape)') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CRTDEVTAP)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Device description')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(10) DFT(*RSRCNAME) +
                          SPCVAL((*RSRCNAME) (*VRT)) KEYPARM(*YES) +
                          PROMPT('Device type')
             PARM       KWD(MODEL) TYPE(*CHAR) LEN(10) DFT(*RSRCNAME) +
                          SPCVAL((*RSRCNAME)) KEYPARM(*YES) +
                          PROMPT('Device model')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*VRT)) KEYPARM(*YES) +
                          PROMPT('Resource name')
             PARM       KWD(SWTSET) TYPE(*CHAR) LEN(2) +
                          PROMPT('Switch setting')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(CTL) TYPE(*NAME) LEN(10) +
                          PROMPT('Attached controller')
             PARM       KWD(ASSIGN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Assign device at vary on')
             PARM       KWD(UNLOAD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Unload device at vary off')
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
