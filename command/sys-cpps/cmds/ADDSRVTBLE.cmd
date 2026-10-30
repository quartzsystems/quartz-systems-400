/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDSRVTBLE: add a service to the services table. */
             CMD        PROMPT('Add Service Table Entry') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(ADDSRVTBLE)
             PARM       KWD(SERVICE) TYPE(*CHAR) LEN(32) MIN(1) CASE(*MIXED) +
                          PROMPT('Service')
             PARM       KWD(PORT) TYPE(*INT4) RANGE(1 65535) MIN(1) +
                          PROMPT('Port')
             PARM       KWD(PROTOCOL) TYPE(*CHAR) LEN(32) MIN(1) CASE(*MIXED) +
                          PROMPT('Protocol')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text')
             PARM       KWD(ALIASES) TYPE(*CHAR) LEN(32) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(4) CASE(*MIXED) PROMPT('Aliases')
