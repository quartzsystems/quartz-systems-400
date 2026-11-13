// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;

/** QMHRTVM RTVM0400 for each ID: severity and substitution variable formats only. */
public final class MsgFmt {
    public static void main(String[] a) throws Exception {
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        AS400Bin4 b4 = new AS400Bin4();
        AS400Text t7 = new AS400Text(7, s), t10 = new AS400Text(10, s), t20 = new AS400Text(20, s);
        for (String id : a) {
            ProgramParameter[] p = new ProgramParameter[] {
                new ProgramParameter(8192),
                new ProgramParameter(b4.toBytes(8192)),
                new ProgramParameter(new AS400Text(8, s).toBytes("RTVM0400")),
                new ProgramParameter(t7.toBytes(id)),
                new ProgramParameter(t20.toBytes("QCPFMSG   *LIBL     ")),
                new ProgramParameter(new byte[1]),
                new ProgramParameter(b4.toBytes(0)),
                new ProgramParameter(t10.toBytes("*NO")),
                new ProgramParameter(t10.toBytes("*NO")),
                new ProgramParameter(new byte[16]),
            };
            ProgramCall pc = new ProgramCall(s, "/QSYS.LIB/QMHRTVM.PGM", p);
            if (!pc.run()) {
                StringBuilder e = new StringBuilder();
                for (AS400Message m : pc.getMessageList()) e.append(m.getID()).append(' ');
                System.out.println(id + " ERR " + e);
                continue;
            }
            byte[] r = p[0].getOutputData();
            int sev = (Integer) b4.toObject(r, 8);
            int n = (Integer) b4.toObject(r, 36);
            int off = (Integer) b4.toObject(r, 88);
            int el = (Integer) b4.toObject(r, 100);
            StringBuilder o = new StringBuilder(id + " " + sev);
            for (int i = 0; i < n; i++) {
                int at = off + i * el;
                int len = (Integer) b4.toObject(r, at);
                int sz = (Integer) b4.toObject(r, at + 4);
                int dec = (Integer) b4.toObject(r, at + 20);
                String ty = ((String) t10.toObject(r, at + 8)).trim();
                o.append(" &").append(i + 1).append("=").append(ty).append(':').append(len).append(':').append(sz).append(':').append(dec);
            }
            System.out.println(o);
        }
        s.disconnectAllServices();
    }
}
