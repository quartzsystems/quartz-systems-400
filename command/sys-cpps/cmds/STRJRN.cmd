/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRJRN: start journaling IFS objects. The attributes are kept; this  */
/* release writes no IFS journal entries (docs/IFS.md).                 */
             CMD        PROMPT('Start Journal') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOJCMD) HLPID(STRJRN)
             PARM       KWD(OBJ) TYPE(E1) MAX(300) PROMPT('Objects')
             PARM       KWD(OBJFID) TYPE(*CHAR) LEN(32) MAX(300) +
                          PROMPT('File identifier')
             PARM       KWD(JRN) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Journal')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
             PARM       KWD(PATTERN) TYPE(E2) MAX(20) PMTCTL(*PMTRQS) +
                          PROMPT('Name pattern')
             PARM       KWD(INHERIT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('New objects inherit journaling')
             PARM       KWD(IMAGES) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*AFTER) VALUES(*AFTER *BOTH) +
                          PMTCTL(*PMTRQS) PROMPT('Images')
             PARM       KWD(OMTJRNE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *OPNCLO) +
                          PMTCTL(*PMTRQS) PROMPT('Omit journal entry')
             PARM       KWD(LOGLVL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ERRORS) VALUES(*ERRORS *ALL) +
                          PMTCTL(*PMTRQS) PROMPT('Logging level')
 E1:         ELEM       TYPE(*PNAME) LEN(5000) PROMPT('Name')
             ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*INCLUDE) +
                          VALUES(*INCLUDE *OMIT) PROMPT('Include or omit')
 E2:         ELEM       TYPE(*CHAR) LEN(255) DFT('*') PROMPT('Pattern')
             ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*INCLUDE) +
                          VALUES(*INCLUDE *OMIT) PROMPT('Include or omit')
