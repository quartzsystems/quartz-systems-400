/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDLICKEY: add a license key; a Quartz key is 18 groups of 6. */
             CMD        PROMPT('Add License Key Information') +
                          MAXPOS(4) ALLOW(*ALL) HLPPNLGRP(QHLZCMD) +
                          HLPID(ADDLICKEY)
             PARM       KWD(LICKEYINP) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*PROMPT) VALUES(*PROMPT *LICKEYFILE) +
                          PROMPT('License key input')
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(7) PMTCTL(P1) +
                          PROMPT('Product identifier')
             PARM       KWD(LICTRM) TYPE(*CHAR) LEN(6) PMTCTL(P1) +
                          PROMPT('License term')
             PARM       KWD(FEATURE) TYPE(*CHAR) LEN(4) PMTCTL(P1) +
                          PROMPT('Feature')
             PARM       KWD(SERIAL) TYPE(*CHAR) LEN(8) DFT(*LOCAL) +
                          SPCVAL((*LOCAL)) PMTCTL(P1) +
                          PROMPT('System serial number')
             PARM       KWD(PRCGRP) TYPE(*CHAR) LEN(4) DFT(*ANY) +
                          SPCVAL((*ANY)) PMTCTL(P1) PROMPT('Processor group')
             PARM       KWD(LICKEY) TYPE(*CHAR) LEN(6) MAX(18) +
                          PMTCTL(P1) PROMPT('License key')
             PARM       KWD(USGLMT) TYPE(*CHAR) LEN(7) DFT(*NOMAX) +
                          SPCVAL((*NOMAX)) PMTCTL(P1) PROMPT('Usage limit')
             PARM       KWD(EXPDATE) TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(P1) PROMPT('Expiration date')
             PARM       KWD(VNDDTA) TYPE(*CHAR) LEN(8) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(P1) PROMPT('Vendor data')
             PARM       KWD(LICKEYFILE) TYPE(*PNAME) LEN(256) +
                          PMTCTL(P2) PROMPT('License key stream file')
             P1:        PMTCTL     CTL(LICKEYINP) COND((*EQ *PROMPT))
             P2:        PMTCTL     CTL(LICKEYINP) COND((*EQ *LICKEYFILE))
