/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDEVMLB: change a media library device description; what is not */
/* given stays as it is.                                              */
             CMD        PROMPT('Change Device Desc (Media Lib)') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CHGDEVMLB)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device description')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*VRT)) PROMPT('Resource name')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(UNLOADWAIT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 600) SPCVAL((*SAME -3) (*SYSGEN 0) +
                          (*NOMAX -1)) PROMPT('Unload wait time')
             PARM       KWD(MAXDEVTIME) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 600) SPCVAL((*SAME -3) (*SYSGEN 0) +
                          (*NOMAX -1)) PROMPT('Maximum device wait time')
             PARM       KWD(GENCTGID) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *VOLID *SYSGEN) +
                          PROMPT('Generate cartridge ids')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*SYSOPR)) +
                          PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
