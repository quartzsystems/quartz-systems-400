/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSIMPCRT: import certificates and keys from a PEM or PKCS#12       */
/* stream file (Quartz extension).                                    */
             CMD        PROMPT('Import Certificate') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(FROMSTMF) TYPE(*PNAME) LEN(5000) MIN(1) +
                          CASE(*MIXED) PROMPT('Stream file to import')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Certificate label')
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) DFT(*SYSTEM) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
             PARM       KWD(FORMAT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*AUTO) VALUES(*AUTO *PEM *PKCS12) +
                          PROMPT('File format')
             PARM       KWD(IMPPWD) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('File password')
             PARM       KWD(TRUST) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Trust imported CAs')
