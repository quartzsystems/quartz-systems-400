/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGIPLA: change the attributes the next IPL uses. */
             CMD        PROMPT('Change IPL Attributes') ALLOW(*ALL) +
                          HLPPNLGRP(QHWMCMD) HLPID(CHGIPLA)
             PARM       KWD(RESTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYS *FULL) +
                          PROMPT('Restart type')
             PARM       KWD(KEYLCKPOS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NORMAL *AUTO *MANUAL +
                          *SECURE) PROMPT('Keylock position')
             PARM       KWD(HDWDIAG) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MIN *ALL) +
                          PROMPT('Hardware diagnostics')
             PARM       KWD(CPRJOBTBL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *NORMAL +
                          *ABNORMAL *ALL) PROMPT('Compress job tables')
             PARM       KWD(CHKJOBTBL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ABNORMAL *ALL *SYNC) +
                          PROMPT('Check job tables')
             PARM       KWD(RBDPRDDIR) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *NORMAL +
                          *ABNORMAL *ALL) PROMPT('Rebuild product directory')
             PARM       KWD(MSFRCY) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *ALL) +
                          PROMPT('Mail Server Framework recovery')
             PARM       KWD(DSPSTS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ALL *NONE *SYS *USER) +
                          PROMPT('Display status')
             PARM       KWD(STRTCP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Start TCP/IP')
             PARM       KWD(SPLFRCY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DETACH *REMOVE) +
                          PROMPT('Spooled file recovery')
             PARM       KWD(CLRJOBQ) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Clear job queues')
             PARM       KWD(CLROUTQ) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Clear output queues')
             PARM       KWD(CLRINCJOB) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Clear incomplete joblogs')
             PARM       KWD(STRPRTWTR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Start print writers')
             PARM       KWD(STRRSTD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Start to restricted state')
