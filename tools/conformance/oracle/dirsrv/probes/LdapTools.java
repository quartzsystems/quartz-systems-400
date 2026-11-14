// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/**
 * The QSH LDAP tools' usage (each run with -? into /home/QSTEST, read back
 * and deleted) and QMHRTVM RTVM0400 for every QGLDMSG ID (severity and
 * substitution formats). Args: OUTDIR IDFILE (the IDs, one per line first
 * word). Usage text goes to OUTDIR only.
 */
public final class LdapTools {
    public static void main(String[] a) throws Exception {
        Path out = Paths.get(a[0]);
        Files.createDirectories(out);
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        CommandCall cc = new CommandCall(s);
        for (String t : new String[] {"ldapsearch", "ldapadd", "ldapmodify", "ldapdelete", "ldapmodrdn", "ldapchangepwd", "ldapexop", "ldapcompare", "ldapdiff"}) {
            String f = "/home/QSTEST/qs" + t + ".txt";
            boolean ok = cc.run("QSH CMD('" + t + " -? > " + f + " 2>&1')");
            try {
                IFSFile ff = new IFSFile(s, f);
                byte[] b = new byte[(int) ff.length()];
                try (IFSFileInputStream in = new IFSFileInputStream(s, f)) { int off = 0; while (off < b.length) { int n = in.read(b, off, b.length - off); if (n < 0) break; off += n; } }
                int ccsid = ff.getCCSID();
                Files.writeString(out.resolve("usage-" + t + ".txt"), new String(b, ccsid == 1208 ? "UTF-8" : ccsid == 819 ? "ISO-8859-1" : String.format("Cp%03d", ccsid)));
                System.out.println(t + " ok=" + ok + " len=" + b.length + " ccsid=" + ccsid);
                ff.delete();
            } catch (Exception e) { System.out.println(t + " ERR " + e); }
        }

        AS400Bin4 b4 = new AS400Bin4();
        AS400Text t7 = new AS400Text(7, s), t10 = new AS400Text(10, s), t20 = new AS400Text(20, s);
        try (PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve("gld-formats.txt")))) {
            for (String line : a.length > 1 ? Files.readAllLines(Paths.get(a[1])) : List.<String>of()) {
                String id = line.trim().split("\\s+")[0];
                if (id.length() != 7) continue;
                ProgramParameter[] p = new ProgramParameter[] {
                    new ProgramParameter(8192), new ProgramParameter(b4.toBytes(8192)),
                    new ProgramParameter(new AS400Text(8, s).toBytes("RTVM0400")), new ProgramParameter(t7.toBytes(id)),
                    new ProgramParameter(t20.toBytes("QGLDMSG   *LIBL     ")), new ProgramParameter(new byte[1]),
                    new ProgramParameter(b4.toBytes(0)), new ProgramParameter(t10.toBytes("*NO")),
                    new ProgramParameter(t10.toBytes("*NO")), new ProgramParameter(new byte[16]),
                };
                ProgramCall pc = new ProgramCall(s, "/QSYS.LIB/QMHRTVM.PGM", p);
                if (!pc.run()) { pw.println(id + " ERR"); continue; }
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
                pw.println(o);
            }
        }
        System.out.println("formats done");
        s.disconnectAllServices();
    }
}
