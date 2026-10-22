/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPMSGD: show message descriptions from a message file. */
             CMD        PROMPT('Display Message Description') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHMHCMD) +
                          HLPID(DSPMSGD)
             PARM       KWD(RANGE) TYPE(E1) DFT(*ALL) SNGVAL((*ALL)) +
                          PROMPT('Range of message identifiers')
             PARM       KWD(MSGF) TYPE(Q1) PROMPT('Message file')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*BASIC) VALUES(*BASIC *FULL) +
                          PROMPT('Detail')
             PARM       KWD(FMTTXT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Format message text')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(7) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Lower value')
                        ELEM       TYPE(*NAME) LEN(7) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*LAST)) PROMPT('Upper value')
