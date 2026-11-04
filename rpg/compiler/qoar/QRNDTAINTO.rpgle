       // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
       // licensed under the terms of the MIT license which can be found in the
       // root directory of this project.
       //
       // QOAR/QRPGLESRC,QRNDTAINTO: what a DATA-INTO parser receives and
       // the procedures it reports the document's names and values with.
       // Its lines are numbered as IBM's member's are, so listings count alike.
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
      /IF DEFINED(QrnDataInto_Included)
      /EOF
      /ENDIF
      /DEFINE QrnDataInto_Included
       // A CCSID of 0 is the job's.
       DCL-C QrnDi_JOB_CCSID 0;
       // The parser's one parameter.
       DCL-DS QrnDiParm_T QUALIFIED TEMPLATE;
          // The document and its length in bytes.
          data POINTER;
          // Set pQrnDiEnv from this to reach the procedures below.
          env POINTER;
          // Passed back on every call.
          handle POINTER;
          // The second operand of %PARSER, if any.
          userParm POINTER;
          dataLen UNS(10);
          // 13488 (UCS-2) unless the options asked for the job's (0).
          dataCcsid UNS(10);
          // The user parameter is a null-terminated string.
          userParmIsNullTermString IND;
          *N CHAR(7);
          *N POINTER DIM(4);
       END-DS;
       // The procedures, by pointer.
       DCL-DS QrnDiEnv QUALIFIED BASED(pQrnDiEnv);
          start POINTER(*PROC);
          finish POINTER(*PROC);
          reportError POINTER(*PROC);
          reportName POINTER(*PROC);
          reportValue POINTER(*PROC);
          startStruct POINTER(*PROC);
          endStruct POINTER(*PROC);
          startArray POINTER(*PROC);
          endArray POINTER(*PROC);
          reportNameCcsid POINTER(*PROC);
          reportValueCcsid POINTER(*PROC);
          trace POINTER(*PROC);
          reportAttr POINTER(*PROC);
          reportAttrCcsid POINTER(*PROC);
          *N POINTER(*PROC) DIM(17);
       END-DS;
       // First, before anything else is reported.
       DCL-PR QrnDiStart EXTPROC(*CWIDEN:QrnDiEnv.start);
          handle POINTER VALUE;
       END-PR;
       // Last: nothing may be reported after it.
       DCL-PR QrnDiFinish EXTPROC(*CWIDEN:QrnDiEnv.finish);
          handle POINTER VALUE;
       END-PR;
       // The document is not valid: an error code above zero and the
       // bytes parsed so far.
       DCL-PR QrnDiReportError EXTPROC(*CWIDEN:QrnDiEnv.reportError);
          handle POINTER VALUE;
          returnCode INT(10) VALUE;
          bytesParsed UNS(10) VALUE;
       END-PR;
       // The name of the next value, structure or array, in the
       // document's CCSID.
       DCL-PR QrnDiReportName EXTPROC(*CWIDEN:QrnDiEnv.reportName);
          handle POINTER VALUE;
          name POINTER VALUE;
          nameLength UNS(10) VALUE;
       END-PR;
       // A name in a CCSID of its own.
       DCL-PR QrnDiReportNameCcsid
                         EXTPROC(*CWIDEN:QrnDiEnv.reportNameCcsid);
          handle POINTER VALUE;
          name POINTER VALUE;
          nameLength UNS(10) VALUE;
          ccsid UNS(10) VALUE;
       END-PR;
       // A scalar value, in the document's CCSID.
       DCL-PR QrnDiReportValue EXTPROC(*CWIDEN:QrnDiEnv.reportValue);
          handle POINTER VALUE;
          val POINTER VALUE;
          valLength UNS(10) VALUE;
       END-PR;
       // A value in a CCSID of its own.
       DCL-PR QrnDiReportValueCcsid
                         EXTPROC(*CWIDEN:QrnDiEnv.reportValueCcsid);
          handle POINTER VALUE;
          val POINTER VALUE;
          valLength UNS(10) VALUE;
          ccsid UNS(10) VALUE;
       END-PR;
       // A structure starts and ends.
       DCL-PR QrnDiStartStruct EXTPROC(*CWIDEN:QrnDiEnv.startStruct);
          handle POINTER VALUE;
       END-PR;
       DCL-PR QrnDiEndStruct EXTPROC(*CWIDEN:QrnDiEnv.endStruct);
          handle POINTER VALUE;
       END-PR;
       // An array starts and ends.
       DCL-PR QrnDiStartArray EXTPROC(*CWIDEN:QrnDiEnv.startArray);
          handle POINTER VALUE;
       END-PR;
       DCL-PR QrnDiEndArray EXTPROC(*CWIDEN:QrnDiEnv.endArray);
          handle POINTER VALUE;
       END-PR;
       // A line for the parser's trace.
       DCL-PR QrnDiTrace EXTPROC(*CWIDEN:QrnDiEnv.trace);
          handle POINTER VALUE;
          msg POINTER VALUE OPTIONS(*STRING);
          nested IND VALUE;
       END-PR;
       // An attribute of the structure being reported.
       DCL-PR QrnDiReportAttr
                         EXTPROC(*CWIDEN:QrnDiEnv.reportAttr);
          handle POINTER VALUE;
          name POINTER VALUE;
          nameLength UNS(10) VALUE;
          value POINTER VALUE;
          valueLength UNS(10) VALUE;
       END-PR;
       // An attribute with CCSIDs of its own.
       DCL-PR QrnDiReportAttrCcsid
                         EXTPROC(*CWIDEN:QrnDiEnv.reportAttrCcsid);
          handle POINTER VALUE;
          name POINTER VALUE;
          nameLength UNS(10) VALUE;
          nameCcsid UNS(10) VALUE;
          value POINTER VALUE;
          valueLength UNS(10) VALUE;
          valueCcsid UNS(10) VALUE;
       END-PR;

