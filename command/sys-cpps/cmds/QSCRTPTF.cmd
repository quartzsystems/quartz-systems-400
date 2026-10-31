/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCRTPTF: build a signed PTF save file from a manifest in the IFS: */
/* the header, the objects on this system, a cover letter.            */
             CMD        PROMPT('Create PTF from Manifest') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(QSCRTPTF)
             PARM       KWD(MANIFEST) TYPE(*PNAME) LEN(1024) MIN(1) +
                          CASE(*MIXED) PROMPT('Manifest stream file')
             PARM       KWD(KEYFILE) TYPE(*PNAME) LEN(1024) MIN(1) +
                          CASE(*MIXED) PROMPT('Signing key file')
             PARM       KWD(TOSAVF) TYPE(Q1) DFT(*PTFID) +
                          SNGVAL((*PTFID)) PROMPT('To save file')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Replace save file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*PTFID)) +
                          MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(QGPL) +
                          PROMPT('Library')
