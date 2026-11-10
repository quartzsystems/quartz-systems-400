// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.sql.*;

/** Read-only: QSYSINC/H QZLS* members (pub400) and the NetServer IFS config directory. */
public final class NsProbe2 {
    public static void main(String[] a) throws Exception {
        boolean pub = a[0].equals("pub400");
        String host = System.getenv(pub ? "QS_PUB400_HOST" : "QS_ORACLE_HOST");
        String user = System.getenv(pub ? "QS_PUB400_USER" : "QS_ORACLE_USER");
        String pass = System.getenv(pub ? "QS_PUB400_PASS" : "QS_ORACLE_PASS");
        String out = a[1];
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        Connection c = DriverManager.getConnection("jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full", user, pass);
        if (pub) {
            String[] mbrs = {"QZLS", "QZLSADFS", "QZLSADPS", "QZLSCHFS", "QZLSCHPS", "QZLSCHSG", "QZLSCHSI", "QZLSCHSN", "QZLSENDS", "QZLSENSS", "QZLSLSTI", "QZLSOLST", "QZLSRMS", "QZLSSTRS"};
            for (String m : mbrs) {
                try (Statement s = c.createStatement()) {
                    try { s.execute("DROP ALIAS QTEMP.ZA"); } catch (Exception e) { }
                    s.execute("CREATE ALIAS QTEMP.ZA FOR QSYSINC.H(" + m + ")");
                    try (ResultSet r = s.executeQuery("SELECT SRCDTA FROM QTEMP.ZA ORDER BY SRCSEQ"); PrintWriter w = new PrintWriter(new FileWriter(out + "/" + m + ".txt"))) {
                        int n = 0;
                        while (r.next()) { w.println(r.getString(1).replaceAll("\\s+$", "")); n++; }
                        System.out.println(m + " lines=" + n);
                    }
                } catch (Exception e) { System.out.println(m + " ERR " + e.getMessage()); }
            }
        }
        AS400 sys = new AS400(host, user, pass);
        for (String d : new String[] {"/QIBM/UserData/OS400/NetServer", "/QIBM/ProdData/OS400/NetServer", "/QIBM/UserData/OS400"}) {
            System.out.println("== " + d);
            try {
                IFSFile f = new IFSFile(sys, d);
                IFSFile[] l = f.listFiles();
                if (l == null) { System.out.println("   (none)"); continue; }
                for (IFSFile x : l) System.out.println("   " + (x.isDirectory() ? "d " : "f ") + x.getName() + " " + x.length() + " ccsid=" + (x.isFile() ? x.getCCSID() : 0));
            } catch (Exception e) { System.out.println("   ERR " + e); }
        }
        sys.disconnectAllServices();
    }
}
