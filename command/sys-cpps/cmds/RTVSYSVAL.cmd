/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVSYSVAL: put a system value into a CL variable. */
             CMD        PROMPT('Retrieve System Value') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVSYSVAL)
             PARM       KWD(SYSVAL) TYPE(*CHAR) LEN(10) RSTD(*YES) MIN(1) +
                          VALUES(QABNORMSW QACGLVL QACTJOB QADLACTJ QADLSPLA +
                          QADLTOTJ QALWJOBITP QALWOBJRST QALWUSRDMN QASTLVL QATNPGM +
                          QAUDCTL QAUDENDACN QAUDFRCLVL QAUDLVL QAUDLVL2 QAUTOCFG +
                          QAUTORMT QAUTOSPRPT QAUTOVRT QBASACTLVL QBASPOOL QCCSID +
                          QCENTURY QCHRID QCHRIDCTL QCMNARB QCMNRCYLMT QCNTRYID +
                          QCONSOLE QCRTAUT QCRTOBJAUD QCTLSBSD QCURSYM QDATE +
                          QDATETIME QDATFMT QDATSEP QDAY QDAYOFWEEK QDBFSTCCOL +
                          QDBRCVYWT QDECFMT QDEVNAMING QDEVRCYACN QDSCJOBITV +
                          QDSPSGNINF QDYNPTYADJ QDYNPTYSCD QENDJOBLMT QFRCCVNRST +
                          QHOUR QHSTLOGSIZ QIGC QIGCCDEFNT QIGCFNTSIZ QINACTITV +
                          QINACTMSGQ QIPLDATTIM QIPLSTS QIPLTYPE QJOBMSGQFL +
                          QJOBMSGQMX QJOBMSGQSZ QJOBMSGQTL QJOBSPLA QKBDBUF +
                          QKBDTYPE QLANGID QLEAPADJ QLIBLCKLVL QLMTDEVSSN +
                          QLMTSECOFR QLOCALE QLOGOUTPUT QMAXACTLVL QMAXJOB +
                          QMAXSGNACN QMAXSIGN QMAXSPLF QMCHPOOL QMINUTE QMODEL +
                          QMONTH QPASTHRSVR QPFRADJ QPRBFTR QPRBHLDITV QPRCFEAT +
                          QPRCMLTTSK QPRTDEV QPRTKEYFMT QPRTTXT QPWDCHGBLK +
                          QPWDEXPITV QPWDEXPWRN QPWDLMTAJC QPWDLMTCHR QPWDLMTREP +
                          QPWDLVL QPWDMAXLEN QPWDMINLEN QPWDPOSDIF QPWDRQDDGT +
                          QPWDRQDDIF QPWDRULES QPWDVLDPGM QPWRDWNLMT QPWRRSTIPL +
                          QQRYDEGREE QQRYTIMLMT QRCLSPLSTG QRETSVRSEC QRMTIPL +
                          QRMTSIGN QRMTSRVATR QSAVACCPTH QSCANFS QSCANFSCTL +
                          QSCPFCONS QSECOND QSECURITY QSETJOBATR QSFWERRLOG +
                          QSHRMEMCTL QSPCENV QSPLFACN QSRLNBR QSRTSEQ QSRVDMP +
                          QSSLCSL QSSLCSLCTL QSSLPCL QSTGLOWACN QSTGLOWLMT +
                          QSTRPRTWTR QSTRUPPGM QSTSMSG QSYSLIBL QSYSNAME QTHDRSCADJ +
                          QTHDRSCAFN QTIMADJ QTIME QTIMSEP QTIMZON QTOTJOB QTSEPOOL +
                          QUPSDLYTIM QUPSMSGQ QUSEADPAUT QUSRLIBL QUTCOFFSET +
                          QVFYOBJRST QYEAR) +
                          PROMPT('System value')
             PARM       KWD(RTNVAR) TYPE(*CHAR) LEN(1) MIN(1) RTNVAL(*YES) +
                          PROMPT('CL var for returned value')
