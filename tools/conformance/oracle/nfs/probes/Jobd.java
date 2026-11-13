// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;

/** The attributes of job descriptions in QSYS, read only. Args: NAME... */
public final class Jobd {
    public static void main(String[] a) throws Exception {
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        for (String n : a) {
            JobDescription d = new JobDescription(s, "QSYS", n);
            System.out.println("JOBD " + n + " user=" + d.getUserName() + " jobq=" + d.getJobQueueLibraryName() + "/" + d.getJobQueueName()
                + " jobpty=" + d.getJobQueuePriority() + " outpty=" + d.getOutputQueuePriority()
                + " outq=" + d.getOutputQueueLibraryName() + "/" + d.getOutputQueueName() + " prtdev=" + d.getPrinterDeviceName()
                + " rtgdta=" + d.getRoutingData() + " libl=" + String.join(",", d.getInitialLibraryList())
                + " log=" + d.getMessageLoggingLevel() + "/" + d.getMessageLoggingSeverity() + "/" + d.getMessageLoggingText()
                + " logclpgm=" + d.isLoggingOfCLPrograms() + " inqmsgrpy=" + d.getInquiryMessageReply() + " hold=" + d.isHoldOnJobQueue()
                + " endsev=" + d.getEndSeverity() + " text=" + d.getTextDescription() + " alwmltthd=" + d.isAllowMultipleThreads()
                + " acgcde=" + d.getAccountingCode() + " joblogout=" + d.getJobLogOutput() + " spla=" + d.getSpooledFileAction());
        }
        s.disconnectAllServices();
    }
}
