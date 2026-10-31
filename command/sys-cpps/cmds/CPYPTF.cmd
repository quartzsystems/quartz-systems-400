/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYPTF: copy PTF save files to a save file or a device. */
             CMD        PROMPT('Copy PTF') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(CPYPTF)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('PTF numbers to select')
             PARM       KWD(TODEV) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*SAVF)) PROMPT('To device')
             PARM       KWD(FROMDEV) TYPE(*NAME) LEN(10) DFT(*SERVICE) +
                          SPCVAL((*SERVICE) (*SAVF)) PROMPT('From device')
             PARM       KWD(TOSAVF) TYPE(Q1) DFT(*PTFID) +
                          SNGVAL((*PTFID)) PROMPT('To save file')
             PARM       KWD(TOSEQNBR) TYPE(*INT4) DFT(*END) RANGE(1 +
                          16777215) SPCVAL((*END -1)) PROMPT('To +
                          sequence number')
             PARM       KWD(TOENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LEAVE) VALUES(*LEAVE *REWIND *UNLOAD) +
                          PROMPT('To device end option')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(CPYPTFCVR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Copy PTF cover letter')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Replace save file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*PTFID)) +
                          MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(QGPL) +
                          PROMPT('Library')
