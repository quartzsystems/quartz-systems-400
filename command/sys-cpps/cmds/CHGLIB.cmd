/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLIB: change a library's type, text and creation defaults. */
             CMD        PROMPT('Change Library') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CHGLIB)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *PROD *TEST) +
                          PROMPT('Library type')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             PARM       KWD(CRTAUT) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*CHANGE) (*ALL) +
                          (*USE) (*EXCLUDE)) +
                          PROMPT('Authority for new objects')
             PARM       KWD(CRTOBJAUD) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NONE +
                          *USRPRF *CHANGE *ALL) +
                          PROMPT('Auditing for new objects')
