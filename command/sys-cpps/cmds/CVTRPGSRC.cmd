/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CVTRPGSRC: convert RPG III source members to ILE RPG (licensed       */
/* program QS00RPG, docs/RPG3.md).                                      */
             CMD        PROMPT('Convert RPG Source') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(CVTRPGSRC)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('From file')
             PARM       KWD(FROMMBR) TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('From member')
             PARM       KWD(TOFILE) TYPE(Q2) SNGVAL((*NONE)) +
                          PROMPT('To file')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FROMMBR) +
                          SPCVAL((*FROMMBR)) PROMPT('To member')
             PARM       KWD(EXPCPY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Expand copy member')
             PARM       KWD(CVTRPT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Print conversion report')
             PARM       KWD(SECLVL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Include second level text')
             PARM       KWD(INSRTPL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Insert specification template')
             PARM       KWD(LOGFILE) TYPE(Q3) SNGVAL((*NONE)) +
                          PROMPT('Log file')
             PARM       KWD(LOGMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*LAST)) PROMPT('Log file member')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QRPGLESRC)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*FROMLIB) +
                          SPCVAL((*FROMLIB) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) DFT(QRNCVTLG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
