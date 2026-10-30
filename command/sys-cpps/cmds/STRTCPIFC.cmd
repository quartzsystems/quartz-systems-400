/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTCPIFC: start one interface, or every autostart one. */
             CMD        PROMPT('Start TCP/IP Interface') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(STRTCPIFC)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) MIN(1) +
                          SPCVAL((*AUTOSTART) (*IP4DHCP) (*IP6SAC)) +
                          PROMPT('Internet address')
