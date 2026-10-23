/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGGRPA: make the job a group job, rename it, or leave the group. */
             CMD        PROMPT('Change Group Attributes') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(CHGGRPA)
             PARM       KWD(GRPJOB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Group job')
             PARM       KWD(GRPJOBTXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Group job text')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Message queue')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
