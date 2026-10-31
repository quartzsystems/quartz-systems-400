/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RST: restore IFS objects. Media arrive in Phase 11: the processing  */
/* program sends CPF3700. The box refused the definition to pub400's   */
/* profile; these are IBM's documented parameters.                     */
             CMD        PROMPT('Restore Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHRSCMD) HLPID(RST)
             PARM       KWD(DEV) TYPE(*PNAME) LEN(5000) MIN(1) +
                          EXPR(*YES) PROMPT('Device')
             PARM       KWD(OBJ) TYPE(E1) MAX(300) EXPR(*YES) +
                          PROMPT('Objects')
             PARM       KWD(PATTERN) TYPE(E2) MAX(300) EXPR(*YES) +
                          PROMPT('Name pattern')
             PARM       KWD(SUBTREE) TYPE(*INT4) RSTD(*YES) DFT(*ALL) +
                          SPCVAL((*ALL -1) (*DIR -2) (*NONE -4) +
                          (*OBJ -3) (*STG -5)) PROMPT('Directory subtree')
             PARM       KWD(SYSTEM) TYPE(*INT4) RSTD(*YES) DFT(*LCL) +
                          SPCVAL((*ALL -1) (*LCL -2) (*RMT -3)) +
                          PMTCTL(*PMTRQS) PROMPT('Restore to system')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *NEW *OLD) +
                          PROMPT('Option')
             PARM       KWD(ALWOBJDIF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *OWNER +
                          *PGP *AUTL) MAX(4) PROMPT('Allow object differences')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) DFT(*MOUNTED) +
                          SNGVAL((*MOUNTED)) MAX(75) EXPR(*YES) +
                          PROMPT('Volume identifier')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*SEARCH) +
                          SPCVAL((*SEARCH)) EXPR(*YES) PROMPT('Label')
             PARM       KWD(OPTFILE) TYPE(*PNAME) LEN(256) DFT('*') +
                          CASE(*MIXED) PROMPT('Optical file')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*SEARCH) +
                          RANGE(1 16777215) SPCVAL((*SEARCH -1)) +
                          EXPR(*YES) PROMPT('Sequence number')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(OUTPUT) TYPE(*PNAME) LEN(5000) DFT(*NONE) +
                          SPCVAL((*NONE) (*PRINT)) EXPR(*YES) +
                          PROMPT('Output')
 E1:         ELEM       TYPE(*PNAME) LEN(5000) DFT('*') EXPR(*YES) +
                          PROMPT('Name')
             ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*INCLUDE) +
                          SPCVAL((*INCLUDE I) (*OMIT O)) +
                          PROMPT('Include or omit')
             ELEM       TYPE(*PNAME) LEN(5000) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('New object name')
 E2:         ELEM       TYPE(*CHAR) LEN(255) DFT(*) SPCVAL((*)) +
                          EXPR(*YES) PROMPT('Pattern')
             ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*INCLUDE) +
                          SPCVAL((*INCLUDE I) (*OMIT O)) +
                          PROMPT('Include or omit')
