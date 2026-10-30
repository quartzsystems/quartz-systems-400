/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVTCPIFC: remove an interface. */
             CMD        PROMPT('Remove TCP/IP Interface') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(RMVTCPIFC)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) +
                          SPCVAL((*IP4DHCP) (*IP6SAC)) +
                          PROMPT('Internet address')
             PARM       KWD(ALIASNAME) TYPE(*CHAR) LEN(25) +
                          PROMPT('Alias name')
             PARM       KWD(LIND) TYPE(E1) DFT(*ADRALIAS) +
                          SNGVAL((*ADRALIAS)) PROMPT('Line')
             E1:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
