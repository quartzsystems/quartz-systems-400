/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCHGCRT: change a certificate's label, trust, default or text     */
/* (Quartz extension).                                                */
             CMD        PROMPT('Change Certificate') MAXPOS(1) +
                          ALLOW(*ALL)
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Certificate label')
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) DFT(*SYSTEM) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
             PARM       KWD(NEWLABEL) TYPE(*CHAR) LEN(64) DFT(*SAME) +
                          SPCVAL((*SAME)) CASE(*MIXED) PROMPT('New label')
             PARM       KWD(TRUST) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Trusted CA')
             PARM       KWD(DFTCRT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Make the store default')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK)) CASE(*MIXED) PROMPT('Text')
