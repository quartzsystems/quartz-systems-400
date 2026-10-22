/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKMSGD: the Work with Message Descriptions display. */
             CMD        PROMPT('Work with Message Descriptions') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHMHCMD) +
                          HLPID(WRKMSGD)
             PARM       KWD(MSGID) TYPE(*GENERIC) LEN(7) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) PROMPT('Message file')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
