/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGNTPA: the SNTP client's and server's attributes, kept in the     */
/* TCP/IP server table in QUSRSYS; QTOTNTP reads them at each poll.    */
/* Keywords, ranges and special values are the oracle's (V7R5M0).      */
             CMD        PROMPT('Change SNTP Attributes') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOTCMD) HLPID(CHGNTPA)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) MAX(3) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Remote system')
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Client autostart')
             PARM       KWD(POLLITV) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 1440) SPCVAL((*SAME -1)) +
                          PROMPT('Client poll interval')
             PARM       KWD(MINADJ) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 300000) SPCVAL((*SAME -1) (*NONE -2)) +
                          PROMPT('Client minimum adjustment')
             PARM       KWD(MAXADJ) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 120) SPCVAL((*SAME -1)) +
                          PROMPT('Client maximum adjustment')
             PARM       KWD(ADJTHLD) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 7200) SPCVAL((*SAME -1) (*MAXADJ -2)) +
                          PROMPT('Client adjustment threshold')
             PARM       KWD(ACTLOG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *CHANGE *POLL) +
                          PROMPT('Client activity log')
             PARM       KWD(SVRAUTOSTR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Server autostart')
             PARM       KWD(SVRACTLOG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *ALL *ERROR) +
                          PROMPT('Server activity log')
             PARM       KWD(SYNCRQD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Synchronization required')
