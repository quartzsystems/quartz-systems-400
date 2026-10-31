/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDPTFORD: order PTFs, groups or a cumulative package from the update */
/* service; they arrive as save files in QGPL.                          */
             CMD        PROMPT('Send PTF Order') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(SNDPTFORD)
             PARM       KWD(PTFID) TYPE(E1) MIN(1) MAX(300) +
                          PROMPT('PTF description')
             PARM       KWD(PTFPART) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *CVRLTR) +
                          PROMPT('PTF parts')
             PARM       KWD(RMTCPNAME) TYPE(*NAME) LEN(8) +
                          DFT(*IBMSRV) SPCVAL((*IBMSRV) (*SELECT)) +
                          PROMPT('Remote control point')
             PARM       KWD(RMTNETID) TYPE(*NAME) LEN(8) +
                          DFT(*NETATR) SPCVAL((*NETATR)) +
                          PROMPT('Remote network identifier')
             PARM       KWD(DLVRYFMT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAVF) VALUES(*SAVF *IMAGE) +
                          PROMPT('Delivery format')
             PARM       KWD(ORDER) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*REQUIRED) VALUES(*REQUIRED *PTFID) +
                          PROMPT('Order')
             PARM       KWD(REORDER) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Reorder')
             PARM       KWD(CHKPTF) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Check PTF')
             PARM       KWD(IMGDIR) TYPE(*PNAME) LEN(1024) +
                          DFT(*DFT) SPCVAL((*DFT)) PMTCTL(P1) +
                          PROMPT('Image directory')
             E1:        ELEM       TYPE(*CHAR) LEN(9) MIN(1) +
                          PROMPT('PTF identifier')
                        ELEM       TYPE(*CHAR) LEN(7) DFT(*ONLYPRD) +
                          SPCVAL((*ONLYPRD)) PROMPT('Product')
                        ELEM       TYPE(*CHAR) LEN(6) DFT(*ONLYRLS) +
                          SPCVAL((*ONLYRLS)) PROMPT('Release')
             P1:        PMTCTL     CTL(DLVRYFMT) COND((*EQ *IMAGE))
