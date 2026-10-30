/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSDLTCRT: remove a certificate from a store (Quartz extension).    */
             CMD        PROMPT('Delete Certificate') MAXPOS(1) +
                          ALLOW(*ALL)
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Certificate label')
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) DFT(*SYSTEM) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
