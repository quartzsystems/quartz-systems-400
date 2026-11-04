       // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
       // licensed under the terms of the MIT license which can be found in the
       // root directory of this project.
       //
       // QOAR/QRPGLESRC,QRNDTAGEN: what a DATA-GEN generator receives for
       // each event and the procedures it writes its text with.
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
      /IF DEFINED(QrnDataGen_Included)
      /EOF
      /ENDIF
      /DEFINE QrnDataGen_Included

      /COPY QOAR/QRPGLESRC,QRNTYPES

       // The procedures, by pointer; set pQrnDgEnv from the parameter's
       // env subfield.
       DCL-DS QrnDgEnv QUALIFIED BASED(pQrnDgEnv);
          reportError POINTER(*PROC);
          trace POINTER(*PROC);
          addText POINTER(*PROC);
          addTextCcsid POINTER(*PROC);
          addTextString POINTER(*PROC);
          addTextNewLine POINTER(*PROC);
          getSubfieldName POINTER(*PROC);
          *N POINTER(*PROC) DIM(13);
       END-DS QrnDgEnv;

       // A CCSID of 0 is the job's.
       DCL-C QrnDg_JOB_CCSID 0;

       // A name, as the generator gets it.
       DCL-S QrnDgName_T VARUCS2(4096:4) CCSID(*UTF16) TEMPLATE;

       // The events, in the parameter's event subfield.
       DCL-C QrnDgEvent_01_StartMultiple     1;
       DCL-C QrnDgEvent_02_EndMultiple       2;
       DCL-C QrnDgEvent_03_Start             3;
       DCL-C QrnDgEvent_04_End               4;
       DCL-C QrnDgEvent_05_StartStruct       5;
       DCL-C QrnDgEvent_06_EndStruct         6;
       DCL-C QrnDgEvent_07_StartScalarArray  7;
       DCL-C QrnDgEvent_08_EndScalarArray    8;
       DCL-C QrnDgEvent_09_StartStructArray  9;
       DCL-C QrnDgEvent_10_EndStructArray    10;
       DCL-C QrnDgEvent_11_ScalarValue       11;
       DCL-C QrnDgEvent_12_Terminate         12;

       // An array: its elements, and for an array of structures the
       // subfields; the subfield number in its parent (0 for none).
       DCL-DS QrnDgArray_T QUALIFIED ALIGN(*FULL) TEMPLATE;
          totalElems UNS(10);
          numSubfields UNS(10);
          subfieldNumber UNS(10);
       END-DS QrnDgArray_T;

       // A data structure: the element (0 when it is no array), the
       // elements, the subfields, its number in its parent, and the
       // file format it was described from.
       DCL-DS QrnDgDs_T QUALIFIED ALIGN(*FULL) TEMPLATE;
          elem UNS(10);
          totalElems UNS(10);
          numSubfields UNS(10);
          subfieldNumber UNS(10);
          isExtDesc IND;
          extLibrary CHAR(10);
          extFile CHAR(10);
          extFormat CHAR(10);
          recordLevelId CHAR(13);
       END-DS QrnDgDs_T;

       // A scalar: where it is, the field's CCSID and type, and its
       // value as text in valueCcsid.
       DCL-DS QrnDgScalar_T QUALIFIED ALIGN(*FULL) TEMPLATE;
          elem UNS(10);
          totalElems UNS(10);
          definedCcsid UNS(10);
          subfieldNumber UNS(10);
          // QrnDatatype_*; varying and fixed are not told apart.
          dataType UNS(3);
          // QrnDtzFormat_*, for dates, times and timestamps.
          dtzFormat UNS(3);
          // The separator, or the decimal point.
          separator CHAR(1);
          *N CHAR(25);
          valueLenBytes UNS(10);
          valueLenChars UNS(10);
          valueCcsid UNS(10);
          *N UNS(10);
          *N UNS(10);
          value POINTER;
          *N POINTER;
          *N POINTER;
       END-DS QrnDgScalar_T;

       // The generator's one parameter.
       DCL-DS QrnDgParm_T QUALIFIED ALIGN(*FULL) TEMPLATE;
          // The generator's own, kept from call to call.
          generatorState POINTER;
          env POINTER;
          // Passed back on every call.
          handle POINTER;
          *N POINTER;
          // The second operand of %GEN, if any.
          userParm POINTER;
          *N UNS(10);
          *N UNS(10);
          userParmSize UNS(10);
          userParmCcsid UNS(10);
          // QrnUserParmType_*.
          userParmType CHAR(1);
          outputIsToFile IND;
          // Set on to be called with the Terminate event at the end.
          doTerminateEvent IND;
          // Part of a *START ... *END sequence.
          isPartOfSequence IND;
          *N CHAR(24);
          // The item's name; blank for events 1 to 4 and 12.
          name LIKE(QrnDgName_T);
          event UNS(10);
          *N UNS(10);
          *N UNS(10);
          *N UNS(10);
          *N POINTER;
          *N POINTER;
          // One of these, by event.
          ds LIKEDS(QrnDgDs_T);
          array LIKEDS(QrnDgArray_T)
                SAMEPOS(ds);
          scalar LIKEDS(QrnDgScalar_T)
                 SAMEPOS(ds);
       END-DS QrnDgParm_T;

       // The data cannot be generated: an error code; the operation ends.
       DCL-PR QrnDgReportError EXTPROC(*CWIDEN:QrnDgEnv.reportError);
          handle POINTER VALUE;
          returnCode UNS(10) VALUE;
       END-PR;

       // A line for the generator's trace.
       DCL-PR QrnDgTrace EXTPROC(*CWIDEN:QrnDgEnv.trace);
          handle POINTER VALUE;
          msg POINTER VALUE OPTIONS(*STRING);
          nested IND VALUE;
       END-PR;

       // Text in UTF-16, a count of characters.
       DCL-PR QrnDgAddText
                         EXTPROC(*CWIDEN:QrnDgEnv.addText);
          handle POINTER VALUE;
          text POINTER VALUE;
          textChars UNS(10) VALUE;
       END-PR;

       // Text in a CCSID, a count of bytes.
       DCL-PR QrnDgAddTextCcsid
                         EXTPROC(*CWIDEN:QrnDgEnv.addTextCcsid);
          handle POINTER VALUE;
          text POINTER VALUE;
          textBytes UNS(10) VALUE;
          ccsid UNS(10) VALUE;
       END-PR;

       // Text ending in a null, in the job's CCSID.
       DCL-PR QrnDgAddTextString
                         EXTPROC(*CWIDEN:QrnDgEnv.addTextString);
          handle POINTER VALUE;
          text POINTER VALUE OPTIONS(*STRING);
       END-PR;

       // A line break.
       DCL-PR QrnDgAddTextNewLine
                         EXTPROC(*CWIDEN:QrnDgEnv.addTextNewLine);
          handle POINTER VALUE;
       END-PR;

       // The name of a subfield of the structure, by number.
       DCL-PR QrnDgGetSubfieldName
                         LIKE(QrnDgName_T)
                         EXTPROC(*CWIDEN:QrnDgEnv.getSubfieldName)
                         RTNPARM;
          handle POINTER VALUE;
          subfieldNum UNS(10) VALUE;
       END-PR;



























































