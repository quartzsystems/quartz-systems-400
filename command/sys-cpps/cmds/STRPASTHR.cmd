/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRPASTHR: 5250 display station pass-through runs over APPC, which  */
/* QS/400 does not have: no remote location is found (CPF8933). TELNET */
/* reaches another system's displays instead.                          */
             CMD        PROMPT('Start Pass-Through') MAXPOS(3) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHSYCMD) HLPID(STRPASTHR)
             PARM       KWD(RMTLOCNAME) TYPE(*CNAME) LEN(8) +
                          SPCVAL((*CNNDEV)) MIN(1) EXPR(*YES) +
                          PROMPT('Remote location')
             PARM       KWD(CNNDEV) TYPE(*NAME) LEN(10) DFT(*LOC) +
                          SNGVAL((*LOC)) MAX(16) EXPR(*YES) PMTCTL(P2) +
                          PROMPT('APPC device')
             PARM       KWD(VRTCTL) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) EXPR(*YES) +
                          PROMPT('Virtual controller')
             PARM       KWD(VRTDEV) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(32) EXPR(*YES) +
                          PROMPT('Virtual display device')
             PARM       KWD(MODE) TYPE(*CNAME) LEN(8) DFT(*NETATR) +
                          SPCVAL((*NETATR)) EXPR(*YES) PMTCTL(P1) +
                          PROMPT('Mode')
             PARM       KWD(LCLLOCNAME) TYPE(*CNAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR)) EXPR(*YES) +
                          PMTCTL(P1) PROMPT('Local location')
             PARM       KWD(RMTNETID) TYPE(*CNAME) LEN(8) DFT(*LOC) +
                          SPCVAL((*LOC) (*NETATR) (*NONE)) EXPR(*YES) +
                          PMTCTL(P1) +
                          PROMPT('Remote network identifier')
             PARM       KWD(SRQ10PGM) TYPE(Q1) DFT(*SRQMNU) +
                          SNGVAL((*SRQMNU)) EXPR(*YES) +
                          PROMPT('System request program')
             PARM       KWD(RMTUSER) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*CURRENT)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) PROMPT('User profile')
             PARM       KWD(RMTPWD) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('User password')
             PARM       KWD(RMTINLPGM) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) SPCVAL((*RMTUSRPRF) (*NONE)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Initial program to call')
             PARM       KWD(RMTINLMNU) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) +
                          SPCVAL((*RMTUSRPRF) (*SIGNOFF)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) PROMPT('Initial menu')
             PARM       KWD(RMTCURLIB) TYPE(*NAME) LEN(10) +
                          DFT(*RMTUSRPRF) SPCVAL((*RMTUSRPRF)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Current library')
             PARM       KWD(PASTHRSCN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) EXPR(*YES) +
                          PMTCTL(*PMTRQS) PROMPT('Display option')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             P1:        PMTCTL     CTL(RMTLOCNAME) COND((*NE *CNNDEV)) +
                          NBRTRUE(*EQ 1)
             P2:        PMTCTL     CTL(RMTLOCNAME) COND((*EQ *CNNDEV)) +
                          NBRTRUE(*EQ 1)
             DEP        CTL((&RMTLOCNAME *EQ *CNNDEV)) +
                          PARM((&CNNDEV *NE *LOC)) NBRTRUE(*EQ 1) +
                          MSGID(CPD8902)
             DEP        CTL((&RMTUSER *EQ *NONE)) +
                          PARM((&RMTPWD *EQ *NONE) +
                          (&RMTINLPGM *EQ *RMTUSRPRF) +
                          (&RMTINLMNU *EQ *RMTUSRPRF) +
                          (&RMTCURLIB *EQ *RMTUSRPRF)) NBRTRUE(*EQ 4) +
                          MSGID(CPD8904)
             DEP        CTL((&VRTCTL *NE *NONE)) +
                          PARM((&VRTDEV *EQ *NONE)) NBRTRUE(*EQ 1) +
                          MSGID(CPD8901)
             DEP        CTL((&RMTLOCNAME *EQ *CNNDEV)) +
                          PARM((&MODE *EQ *NETATR) +
                          (&LCLLOCNAME *EQ *LOC) (&RMTNETID *EQ *LOC)) +
                          NBRTRUE(*EQ 3) MSGID(CPD8903)
             DEP        CTL(*ALWAYS) PARM((&MODE *NE SNASVCMG) +
                          (&MODE *NE CPSVCMG)) NBRTRUE(*EQ 2) +
                          MSGID(CPD2707)
