/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTDEVMLB: create a media library device description.            */
             CMD        PROMPT('Create Device Desc (Media Lib)') +
                          MAXPOS(4) ALLOW(*ALL) HLPPNLGRP(QHDCDEV) +
                          HLPID(CRTDEVMLB)
             PARM       KWD(DEVD) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Device description')
             PARM       KWD(DEVCLS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          MIN(1) VALUES(*TAP *OPT) KEYPARM(*YES) +
                          PROMPT('Device class')
             PARM       KWD(RSRCNAME) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*VRT) (*NONE)) KEYPARM(*YES) +
                          PROMPT('Resource name')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(9) DFT(*RSRCNAME) +
                          SPCVAL((*RSRCNAME)) KEYPARM(*YES) +
                          PROMPT('Device type')
             PARM       KWD(ONLINE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Online at IPL')
             PARM       KWD(UNLOADWAIT) TYPE(*INT4) DFT(*SYSGEN) +
                          RANGE(1 600) SPCVAL((*SYSGEN 0) (*NOMAX -1)) +
                          PROMPT('Unload wait time')
             PARM       KWD(MAXDEVTIME) TYPE(*INT4) DFT(*SYSGEN) +
                          RANGE(1 600) SPCVAL((*SYSGEN 0) (*NOMAX -1)) +
                          PROMPT('Maximum device wait time')
             PARM       KWD(RSCALCPTY) TYPE(*INT2) DFT(*JOB) +
                          RANGE(1 99) SPCVAL((*JOB 0)) +
                          PROMPT('Resource allocation priority')
             PARM       KWD(INLMNTWAIT) TYPE(*INT4) DFT(*JOB) +
                          RANGE(1 600) SPCVAL((*JOB 0) (*IMMED -2) +
                          (*NOMAX -1)) PROMPT('Initial mount wait time')
             PARM       KWD(EOVMNTWAIT) TYPE(*INT4) DFT(*JOB) +
                          RANGE(1 600) SPCVAL((*JOB 0) (*IMMED -2) +
                          (*NOMAX -1)) +
                          PROMPT('End of volume mount wait time')
             PARM       KWD(GENCTGID) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*VOLID) VALUES(*VOLID *SYSGEN) +
                          PROMPT('Generate cartridge ids')
             PARM       KWD(ROBOTDEV) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) +
                          PROMPT('Robot device descriptions')
             PARM       KWD(ROBOTHOST) TYPE(*CHAR) LEN(255) +
                          DFT(*NONE) SPCVAL((*NONE)) +
                          PROMPT('Robot host')
             PARM       KWD(LCLINTNETA) TYPE(*CHAR) LEN(15) +
                          DFT(*NONE) SPCVAL((*NONE)) +
                          PROMPT('Local internet address')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SYSOPR) +
                          SNGVAL((*SYSOPR)) PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*CHANGE) +
                          SPCVAL((*CHANGE) (*ALL) (*USE) (*EXCLUDE) +
                          (*LIBCRTAUT)) PMTCTL(*PMTRQS) +
                          PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
