/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAV: save IFS objects. The walk is done; media arrive in Phase 11,  */
/* so the command ends with CPF3700 having saved nothing.              */
             CMD        PROMPT('Save Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(SAV)
             PARM       KWD(DEV) TYPE(*PNAME) LEN(5000) MIN(1) MAX(4) +
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
                          PMTCTL(*PMTRQS) PROMPT('Save from system')
             PARM       KWD(PRECHK) TYPE(*INT4) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) PMTCTL(*PMTRQS) +
                          PROMPT('Object pre-check')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             PARM       KWD(UPDHST) TYPE(*INT4) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*SYS -1) (*PC -2)) SNGVAL((*NO 0) +
                          (*YES 1)) MAX(2) PMTCTL(*PMTRQS) +
                          PROMPT('Update history')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) DFT(*MOUNTED) +
                          SNGVAL((*MOUNTED)) MAX(75) EXPR(*YES) +
                          PROMPT('Volume identifier')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*GEN) +
                          SPCVAL((*GEN)) EXPR(*YES) PROMPT('Label')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*END) +
                          RANGE(1 16777215) SPCVAL((*END -3)) +
                          EXPR(*YES) PROMPT('Sequence number')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(CLEAR) TYPE(*INT4) RSTD(*YES) DFT(*NONE) +
                          SPCVAL((*NONE 0) (*ALL -1) (*AFTER -2) +
                          (*REPLACE -3)) PMTCTL(*PMTRQS) +
                          PROMPT('Clear')
             PARM       KWD(DTACPR) TYPE(*INT4) RSTD(*YES) DFT(*DEV) +
                          SPCVAL((*YES 1) (*NO 0) (*DEV -1) (*LOW -2) +
                          (*MEDIUM -3) (*HIGH -4) (*ZLIB -5)) +
                          PMTCTL(*PMTRQS) PROMPT('Data compression')
             PARM       KWD(SAVACT) TYPE(*INT4) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1) (*SYNC -1)) +
                          PROMPT('Save active')
             PARM       KWD(OUTPUT) TYPE(*PNAME) LEN(5000) DFT(*NONE) +
                          SPCVAL((*NONE) (*PRINT)) EXPR(*YES) +
                          PROMPT('Output')
 E1:         ELEM       TYPE(*PNAME) LEN(5000) DFT('*') EXPR(*YES) +
                          PROMPT('Name')
             ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*INCLUDE) +
                          SPCVAL((*INCLUDE I) (*OMIT O)) +
                          PROMPT('Include or omit')
 E2:         ELEM       TYPE(*CHAR) LEN(255) DFT(*) SPCVAL((*)) +
                          EXPR(*YES) PROMPT('Pattern')
             ELEM       TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*INCLUDE) +
                          SPCVAL((*INCLUDE I) (*OMIT O)) +
                          PROMPT('Include or omit')
