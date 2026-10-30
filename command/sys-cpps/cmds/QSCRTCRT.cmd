/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSCRTCRT: create a certificate with a new key: self-signed, signed */
/* by the local CA, or a request (Quartz extension).                  */
             CMD        PROMPT('Create Certificate') MAXPOS(2) +
                          ALLOW(*ALL)
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Certificate label')
             PARM       KWD(CN) TYPE(*CHAR) LEN(64) MIN(1) +
                          CASE(*MIXED) PROMPT('Common name')
             PARM       KWD(KEYSTORE) TYPE(*PNAME) LEN(256) DFT(*SYSTEM) +
                          SPCVAL((*SYSTEM) (*LCLCA)) CASE(*MIXED) +
                          PROMPT('Certificate store')
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Store password')
             PARM       KWD(SIGNER) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SELF) VALUES(*SELF *LCLCA *CSR) +
                          PROMPT('Signed by')
             PARM       KWD(USAGE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SERVER) VALUES(*SERVER *CLIENT) +
                          PROMPT('Certificate use')
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
             PARM       KWD(ALTNAME) TYPE(*CHAR) LEN(255) DFT(*CN) +
                          SNGVAL((*CN) (*NONE)) MAX(10) CASE(*MIXED) +
                          PROMPT('Alternative names')
             PARM       KWD(KEYALG) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ECDSAP256) VALUES(*ECDSAP256 *ECDSAP384 +
                          *RSA2048 *RSA3072 *RSA4096) PROMPT('Key algorithm')
             PARM       KWD(VALIDITY) TYPE(*INT4) DFT(365) +
                          RANGE(1 7300) PROMPT('Validity in days')
             PARM       KWD(CSR) TYPE(*PNAME) LEN(256) DFT(*NONE) +
                          SPCVAL((*NONE)) CASE(*MIXED) +
                          PROMPT('Request stream file')
             PARM       KWD(LCLCAPWD) TYPE(*CHAR) LEN(128) DFT(*STASH) +
                          SPCVAL((*STASH)) CASE(*MIXED) DSPINPUT(*NO) +
                          PROMPT('Local CA password')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) CASE(*MIXED) PROMPT('Text')
