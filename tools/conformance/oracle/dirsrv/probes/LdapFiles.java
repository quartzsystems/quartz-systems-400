// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.charset.*;
import java.nio.file.*;
import java.sql.*;

/**
 * Copies the directory server's configuration files and the QSYSINC/H
 * LDAP members from the primary into OUTDIR (kept out of the tree; the
 * README records only names and shapes). Only QTEMP is written. Args: OUTDIR.
 */
public final class LdapFiles {
    public static void main(String[] a) throws Exception {
        Path out = Paths.get(a[0]);
        Files.createDirectories(out);
        String host = System.getenv("QS_ORACLE_HOST"), user = System.getenv("QS_ORACLE_USER"), pass = System.getenv("QS_ORACLE_PASS");
        AS400 s = new AS400(host, user, pass);
        for (String p : new String[] {
                "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf",
                "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/slapd.version",
                "/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif",
                "/QIBM/UserData/OS400/DirSrv/ldap_server_info.conf",
                "/QIBM/ProdData/OS400/DirSrv/ibmldap.conf",
                "/QIBM/ProdData/OS400/DirSrv/slapd.version",
                "/QIBM/ProdData/OS400/DirSrv/ldapdiff.qsh"}) {
            try {
                IFSFile f = new IFSFile(s, p);
                int ccsid = f.getCCSID();
                byte[] b = new byte[(int) f.length()];
                try (IFSFileInputStream in = new IFSFileInputStream(s, p)) { int off = 0; while (off < b.length) { int n = in.read(b, off, b.length - off); if (n < 0) break; off += n; } }
                String text = new String(b, ccsid == 1208 ? "UTF-8" : ccsid == 819 || ccsid == 437 ? "ISO-8859-1" : String.format("Cp%03d", ccsid));
                Files.writeString(out.resolve(p.substring(p.lastIndexOf('/') + 1)), text);
                System.out.println(p + " ccsid=" + ccsid + " len=" + b.length);
            } catch (Exception e) { System.out.println(p + " ERR " + e); }
        }
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        try (Connection c = DriverManager.getConnection("jdbc:as400://" + host + ";naming=sql;prompt=false", user, pass); Statement st = c.createStatement()) {
            for (String m : new String[] {"LDAP", "LBER", "LDAPSSL", "QGLDPAPI", "QGLDUAPI", "QGLDCPYVL", "QGLDSSDD"}) {
                try {
                    st.execute("CREATE OR REPLACE ALIAS QTEMP.M" + m + " FOR QSYSINC.H(" + m + ")");
                    try (ResultSet r = st.executeQuery("SELECT SRCDTA FROM QTEMP.M" + m + " ORDER BY SRCSEQ"); PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve("H-" + m + ".h")))) {
                        int n = 0;
                        while (r.next()) { pw.println(r.getString(1).replaceAll("\\s+$", "")); n++; }
                        System.out.println("QSYSINC/H(" + m + ") lines=" + n);
                    }
                } catch (Exception e) { System.out.println("H(" + m + ") ERR " + e.getMessage()); }
            }
        }
        s.disconnectAllServices();
    }
}
