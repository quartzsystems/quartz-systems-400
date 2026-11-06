/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTTIMZON: create a time zone description in QSYS. The daylight  */
/* saving rule's parameters are prompted once DSTNAME names one.    */
             CMD        PROMPT('Create Time Zone Description') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHWCCMD) +
                          HLPID(CRTTIMZON)
             PARM       KWD(TIMZON) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Time zone description')
             PARM       KWD(OFFSET) TYPE(*INT2) MIN(1) +
                          RANGE(-779 779) +
                          CHOICE('-779 to 779 minutes') +
                          PROMPT('Offset')
             PARM       KWD(STDNAME) TYPE(E1) PROMPT('Standard Time')
             PARM       KWD(DSTNAME) TYPE(E2) PROMPT('Daylight Saving +
                          Time (DST)')
             PARM       KWD(STDMSG) TYPE(*NAME) LEN(7) PMTCTL(PSTD) +
                          PROMPT('Standard Time message')
             PARM       KWD(DSTMSG) TYPE(*NAME) LEN(7) PMTCTL(PDSTMSG) +
                          PROMPT('DST message')
             PARM       KWD(MSGF) TYPE(Q1) PMTCTL(PMSG) +
                          PROMPT('Message file')
             PARM       KWD(DSTSTR) TYPE(E3) PMTCTL(PDST) +
                          PROMPT('Daylight Saving Time start')
             PARM       KWD(DSTEND) TYPE(E3) PMTCTL(PDST) +
                          PROMPT('Daylight Saving Time end')
             PARM       KWD(DSTSHIFT) TYPE(*INT2) DFT(60) +
                          RANGE(0 120) CHOICE('0 to 120 minutes') +
                          PMTCTL(PDST) PROMPT('Daylight Saving Time +
                          shift')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(YEAROFS) TYPE(*INT2) DFT(0) +
                          RANGE(-140 140) CHOICE('-140 to 140 years') +
                          PMTCTL(*PMTRQS) PROMPT('Year offset')
             PARM       KWD(ALTNAME) TYPE(*CHAR) LEN(132) +
                          CASE(*MIXED) PMTCTL(*PMTRQS) +
                          PROMPT('Alternate name')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*USE) +
                          SPCVAL((*USE) (*LIBCRTAUT) (*CHANGE) (*ALL) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
 E1:         ELEM       TYPE(*CHAR) LEN(10) DFT(*GEN) +
                          SPCVAL((*GEN) (*MSG)) CASE(*MIXED) +
                          CHOICE('Character value, *GEN, *MSG') +
                          PROMPT('Abbreviated name')
             ELEM       TYPE(*CHAR) LEN(50) CASE(*MIXED) +
                          PROMPT('Full name')
 E2:         ELEM       TYPE(*CHAR) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*MSG)) CASE(*MIXED) +
                          CHOICE('Character value, *NONE...') +
                          PROMPT('Abbreviated name')
             ELEM       TYPE(*CHAR) LEN(50) CASE(*MIXED) +
                          PROMPT('Full name')
 E3:         ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) VALUES(*JAN +
                          *FEB *MAR *APR *MAY *JUN *JUL *AUG *SEP +
                          *OCT *NOV *DEC) PROMPT('Month')
             ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) VALUES(*MON +
                          *TUE *WED *THU *FRI *SAT *SUN) PROMPT('Day')
             ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) VALUES(*LAST +
                          1 2 3 4) PROMPT('Relative day of month')
             ELEM       TYPE(*TIME) PROMPT('Time')
 Q1:         QUAL       TYPE(*NAME) LEN(10) DFT(QCPFMSG)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 PSTD:       PMTCTL     CTL(STDNAME) COND((*EQ *MSG))
 PDSTMSG:    PMTCTL     CTL(DSTNAME) COND((*EQ *MSG))
 PMSG:       PMTCTL     CTL(STDNAME) COND((*EQ *MSG))
             PMTCTL     CTL(DSTNAME) COND((*EQ *MSG)) LGLREL(*OR)
 PDST:       PMTCTL     CTL(DSTNAME) COND((*NE *NONE))
