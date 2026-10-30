/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* LPR: send a spooled file to a remote system's line printer        */
/* daemon; the same command as SNDTCPSPLF, under its other name.     */
/* Keywords, types and special values are the oracle's.               */
             CMD        PROMPT('Send TCP/IP Spooled File') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(LPR)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) MIN(1) +
                          SPCVAL((*INTNETADR)) PROMPT('Remote system')
             PARM       KWD(PRTQ) TYPE(*CHAR) LEN(255) MIN(1) +
                          PROMPT('Printer queue')
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Spooled file')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(SPLNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0) (*LAST -1) +
                          (*ANY -2)) PROMPT('Spooled file number')
             PARM       KWD(JOBSYSNAME) TYPE(*NAME) LEN(8) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*CURRENT) (*ANY)) +
                          PROMPT('Job system name')
             PARM       KWD(CRTDATE) TYPE(EC) DFT(*ONLY) +
                          SNGVAL((*ONLY) (*LAST)) +
                          PROMPT('Spooled file created')
             PARM       KWD(DESTTYP) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*OTHER) VALUES(*AS400 *PSF2 *OTHER) +
                          PROMPT('Destination type')
             PARM       KWD(TRANSFORM) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Transform SCS to ASCII')
             PARM       KWD(USRDTATFM) TYPE(QU) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User data transform')
             PARM       KWD(MFRTYPMDL) TYPE(*CHAR) LEN(15) +
                          DFT(*IBM42011) PMTCTL(*PMTRQS) +
                          PROMPT('Manufacturer type and model')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(15) +
                          PMTCTL(*PMTRQS) PROMPT('Internet address')
             PARM       KWD(WSCST) TYPE(QU) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Workstation customizing object')
             PARM       KWD(DLTSPLF) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Delete file after sending')
             PARM       KWD(DESTOPT) TYPE(*CHAR) LEN(128) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Destination-dependent options')
             PARM       KWD(SEPPAGE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Print separator page')
             QJ:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
             QU:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             EC:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Creation date')
                        ELEM       TYPE(*TIME) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*LAST)) PROMPT('Creation time')
