/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCRTCRTS: create a certificate store (Quartz extension).          */
/* KEYSTORE(*LCLCA) makes the local CA.                               */
             CMD        PROMPT('Create Certificate Store') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) MIN(1) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) MIN(1) +
                          CASE(*MIXED) DSPINPUT(*NO) PROMPT('Store password')
             PARM       KWD(STASH) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Keep the password for servers')
             PARM       KWD(ROOTS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Add the shipped root CAs')
             PARM       KWD(CN) TYPE(*CHAR) LEN(64) DFT(*GEN) +
                          SPCVAL((*GEN)) CASE(*MIXED) +
                          PROMPT('Local CA common name')
             PARM       KWD(ORG) TYPE(*CHAR) LEN(64) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PROMPT('Organization')
             PARM       KWD(ORGUNIT) TYPE(*CHAR) LEN(64) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) +
                          PROMPT('Organizational unit')
             PARM       KWD(LOCALITY) TYPE(*CHAR) LEN(64) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) PROMPT('Locality')
             PARM       KWD(STATE) TYPE(*CHAR) LEN(64) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) +
                          PROMPT('State or province')
             PARM       KWD(COUNTRY) TYPE(*CHAR) LEN(5) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Country code')
             PARM       KWD(KEYALG) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ECDSAP256) VALUES(*ECDSAP256 *ECDSAP384 +
                          *RSA2048 *RSA3072 *RSA4096) PROMPT('Key algorithm')
             PARM       KWD(VALIDITY) TYPE(*INT4) DFT(3650) +
                          RANGE(1 7300) PROMPT('Local CA validity in days')
