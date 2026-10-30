/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSEXPCRT: export a certificate, with its key, or a request to a    */
/* stream file (Quartz extension).                                    */
             CMD        PROMPT('Export Certificate') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Certificate label')
             PARM       KWD(TOSTMF) TYPE(*PNAME) LEN(5000) MIN(1) +
                          CASE(*MIXED) PROMPT('Stream file to write')
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) DFT(*SYSTEM) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
             PARM       KWD(FORMAT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PEM) VALUES(*PEM *PKCS12 *CSR) +
                          PROMPT('File format')
             PARM       KWD(INCKEY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Include the private key')
             PARM       KWD(EXPPWD) TYPE(*CHAR) LEN(128) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('File password')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Replace the stream file')
