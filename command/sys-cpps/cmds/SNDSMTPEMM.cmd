/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDSMTPEMM: send an e-mail through the mail router CHGSMTPA names    */
/* (net/mail). The oracle refused the command to its profile, so the   */
/* keywords are IBM's documented ones, unverified here                 */
/* (docs/NET_OPEN_QUESTIONS.md).                                        */
             CMD        PROMPT('Send SMTP E-mail Message') MAXPOS(5) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(SNDSMTPEMM)
             PARM       KWD(RCP) TYPE(ER) MIN(1) MAX(300) +
                          PROMPT('Recipient')
             PARM       KWD(SUBJECT) TYPE(*CHAR) LEN(256) MIN(1) +
                          PROMPT('Subject')
             PARM       KWD(NOTE) TYPE(*CHAR) LEN(5000) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Note')
             PARM       KWD(ATTACH) TYPE(EA) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(300) PROMPT('Attachment')
             PARM       KWD(CONTENT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*PLAIN) VALUES(*PLAIN *HTML) +
                          PROMPT('Content type')
             PARM       KWD(SMIME) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *SIGN *ENCRYPT *BOTH) +
                          PMTCTL(*PMTRQS) PROMPT('S/MIME')
             ER:        ELEM       TYPE(*CHAR) LEN(256) MIN(1) +
                          PROMPT('Recipient address')
                        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*PRI) VALUES(*PRI *CC *BCC) +
                          PROMPT('Recipient type')
             EA:        ELEM       TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Attachment name')
                        ELEM       TYPE(*CHAR) LEN(256) DFT(*OCTET) +
                          SPCVAL((*OCTET) (*PLAIN) (*HTML) (*XML)) +
                          PROMPT('Content type')
                        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*BIN) VALUES(*BIN *TXT) +
                          PROMPT('Attachment type')
