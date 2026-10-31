/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCNTINF: who the service provider reaches, and where. */
             CMD        PROMPT('Change Contact Information') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(CHGCNTINF)
             PARM       KWD(CMPNY) TYPE(*CHAR) LEN(36) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Company')
             PARM       KWD(CONTACT) TYPE(*CHAR) LEN(36) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Contact')
             PARM       KWD(TELNBR) TYPE(TEL) PROMPT('Telephone number')
             PARM       KWD(FAXTELNBR) TYPE(TEL) PROMPT('Fax telephone number')
             PARM       KWD(MAILADDR) TYPE(ADDR) PROMPT('Mailing address')
             PARM       KWD(LNGVER) TYPE(*CHAR) LEN(4) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('National language version')
             PARM       KWD(MEDPTF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *AUTOMATIC *OPTICAL +
                          *TAPE) PROMPT('Media for mailing fixes')
             PARM       KWD(EMAIL) TYPE(*CHAR) LEN(64) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('E-mail address')
 TEL:        ELEM       TYPE(*CHAR) LEN(30) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Primary')
             ELEM       TYPE(*CHAR) LEN(30) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Alternative')
 ADDR:       ELEM       TYPE(*CHAR) LEN(36) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Street address')
             ELEM       TYPE(*CHAR) LEN(36) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('City/State')
             ELEM       TYPE(*CHAR) LEN(20) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Country or region')
             ELEM       TYPE(*CHAR) LEN(12) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Postal code')
