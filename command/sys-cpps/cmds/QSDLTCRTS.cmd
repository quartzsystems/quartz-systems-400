/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSDLTCRTS: delete a certificate store and its stash (Quartz        */
/* extension).                                                        */
             CMD        PROMPT('Delete Certificate Store') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) MIN(1) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
