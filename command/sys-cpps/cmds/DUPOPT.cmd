/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DUPOPT: copy one optical volume onto another.                      */
             CMD        PROMPT('Duplicate Optical') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) HLPID(DUPOPT)
             PARM       KWD(FROMVOL) TYPE(*CHAR) LEN(32) MIN(1) +
                          SPCVAL((*MOUNTED)) PROMPT('From volume identifier')
             PARM       KWD(TOVOL) TYPE(*CHAR) LEN(32) MIN(1) +
                          SPCVAL((*MOUNTED)) PROMPT('To volume identifier')
             PARM       KWD(NEWVOL) TYPE(*CHAR) LEN(32) DFT(*TOVOL) +
                          SPCVAL((*TOVOL) (*FROMVOL)) +
                          PROMPT('New volume identifier')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) PROMPT('Clear')
             PARM       KWD(FROMDEV) TYPE(*NAME) LEN(10) +
                          PROMPT('From device')
             PARM       KWD(TODEV) TYPE(*NAME) LEN(10) +
                          PROMPT('To device')
             PARM       KWD(ALWMEDERR) TYPE(*INT4) DFT(*NONE) +
                          RANGE(1 1000000) SPCVAL((*NONE 0)) +
                          PROMPT('Allow media errors')
             PARM       KWD(FROMENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LEAVE) VALUES(*LEAVE *UNLOAD) +
                          PMTCTL(*PMTRQS) PROMPT('From end of media option')
             PARM       KWD(TOENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LEAVE) VALUES(*LEAVE *UNLOAD) +
                          PMTCTL(*PMTRQS) PROMPT('To end of media option')
