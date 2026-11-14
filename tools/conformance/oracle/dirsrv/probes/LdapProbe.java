// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.net.*;
import java.nio.file.*;
import java.sql.*;
import java.util.*;
import javax.naming.*;
import javax.naming.directory.*;

/**
 * Read-only directory server probe. Args: primary|pub400 OUTDIR. Only QTEMP
 * is written on the primary, nothing on pub400. LDAP is read anonymously
 * (the root DSE and the schema subentry), never bound with a password.
 * Message text, command XML and file bytes go to OUTDIR (kept out of the
 * tree); stdout holds IDs, names and attributes only.
 */
public final class LdapProbe {
    static String host, user, pass;
    static Connection c;
    static Path out;
    static boolean pub;

    static void q(String title, String sql) {
        System.out.println("== " + title);
        try (Statement s = c.createStatement(); ResultSet r = s.executeQuery(sql)) {
            ResultSetMetaData m = r.getMetaData();
            StringBuilder h = new StringBuilder("   |");
            for (int i = 1; i <= m.getColumnCount(); i++) h.append(m.getColumnName(i)).append('|');
            System.out.println(h);
            int n = 0;
            while (r.next() && n < 2000) {
                StringBuilder b = new StringBuilder("   |");
                for (int i = 1; i <= m.getColumnCount(); i++) { String v = r.getString(i); b.append(v == null ? "-" : v.trim()).append('|'); }
                System.out.println(b);
                n++;
            }
            System.out.println("   rows=" + n);
        } catch (Exception e) { System.out.println("   ERR " + e.getMessage()); }
    }

    static void x(String sql) {
        try (Statement s = c.createStatement()) { s.execute(sql); } catch (Exception e) { System.out.println("   ERR(x) " + sql + " : " + e.getMessage()); }
    }

    static void cmd(String cl) {
        String esc = cl.replace("'", "''");
        x(String.format("CALL QSYS.QCMDEXC('%s', %015.5f)", esc, (double) cl.length()));
    }

    static boolean port(int p) {
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, p), 4000);
            System.out.println("   tcp " + p + " open");
            return true;
        } catch (Exception e) { System.out.println("   tcp " + p + " " + e.getMessage()); return false; }
    }

    static void ldap(String base, String filter, int scope, String[] attrs) {
        Hashtable<String, String> env = new Hashtable<>();
        env.put(Context.INITIAL_CONTEXT_FACTORY, "com.sun.jndi.ldap.LdapCtxFactory");
        env.put(Context.PROVIDER_URL, "ldap://" + host + ":389");
        env.put(Context.SECURITY_AUTHENTICATION, "none");
        env.put("com.sun.jndi.ldap.connect.timeout", "5000");
        env.put("java.naming.ldap.attributes.binary", "");
        System.out.println("== ldap base='" + base + "' filter=" + filter);
        try {
            DirContext ctx = new InitialDirContext(env);
            SearchControls sc = new SearchControls();
            sc.setSearchScope(scope);
            sc.setReturningAttributes(attrs);
            sc.setCountLimit(200);
            NamingEnumeration<SearchResult> en = ctx.search(base, filter, sc);
            int n = 0;
            try (PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve("ldap-" + (base.isEmpty() ? "rootdse" : base.replaceAll("[^A-Za-z0-9]", "_")) + ".ldif")))) {
                while (en.hasMore()) {
                    SearchResult r = en.next();
                    pw.println("dn: " + r.getNameInNamespace());
                    System.out.println("   dn: " + r.getNameInNamespace());
                    NamingEnumeration<? extends Attribute> ae = r.getAttributes().getAll();
                    while (ae.hasMore()) {
                        Attribute a = ae.next();
                        System.out.println("      " + a.getID() + " (" + a.size() + " values)");
                        for (int i = 0; i < a.size(); i++) pw.println(a.getID() + ": " + a.get(i));
                    }
                    pw.println();
                    n++;
                }
            } catch (Exception e) { System.out.println("   ERR(enum) " + e); }
            System.out.println("   entries=" + n);
            ctx.close();
        } catch (Exception e) { System.out.println("   ERR " + e); }
    }

    static void file(AS400 s, String path, boolean keepBytes) {
        try {
            IFSFile f = new IFSFile(s, path);
            if (!f.exists()) { System.out.println("   " + path + " absent"); return; }
            System.out.println("   " + path + " len=" + f.length() + " ccsid=" + f.getCCSID() + " owner=" + f.getOwnerName() + " dir=" + f.isDirectory());
            if (f.isDirectory()) {
                String[] l = f.list();
                if (l != null) for (String n : l) System.out.println("      " + n);
                return;
            }
            if (!keepBytes) return;
            byte[] b = new byte[(int) Math.min(f.length(), 262144)];
            try (IFSFileInputStream in = new IFSFileInputStream(s, path)) { int n = in.read(b); if (n < 0) n = 0; b = Arrays.copyOf(b, n); }
            Files.write(out.resolve(path.replace('/', '_') + ".bin"), b);
        } catch (Exception e) { System.out.println("   " + path + " ERR " + e); }
    }

    public static void main(String[] a) throws Exception {
        pub = a[0].equals("pub400");
        out = Paths.get(a[1]);
        Files.createDirectories(out);
        host = System.getenv(pub ? "QS_PUB400_HOST" : "QS_ORACLE_HOST");
        user = System.getenv(pub ? "QS_PUB400_USER" : "QS_ORACLE_USER");
        pass = System.getenv(pub ? "QS_PUB400_PASS" : "QS_ORACLE_PASS");
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        c = DriverManager.getConnection("jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full", user, pass);
        AS400 s = new AS400(host, user, pass);

        System.out.println("== ports");
        boolean ldapOpen = port(389);
        port(636);
        if (ldapOpen) {
            ldap("", "(objectClass=*)", SearchControls.OBJECT_SCOPE, new String[] {"*", "+"});
            ldap("cn=schema", "(objectClass=*)", SearchControls.OBJECT_SCOPE, new String[] {"*", "+"});
        }

        q("release", "SELECT OS_VERSION, OS_RELEASE FROM SYSIBMADM.ENV_SYS_INFO");
        if (!pub) {
            cmd("DSPOBJD OBJ(QSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/SOBJ)");
            q("QSYS objects about the directory", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.SOBJ WHERE ODOBNM LIKE 'QGLD%' OR ODOBNM LIKE 'QDIR%' OR ODOBNM LIKE '%LDAP%' OR ODOBNM LIKE 'QSYDIR%' OR UPPER(ODOBTX) LIKE '%LDAP%' OR UPPER(ODOBTX) LIKE '%DIRECTORY SERV%' OR UPPER(ODOBTX) LIKE '%DIRSRV%' OR UPPER(ODOBTX) LIKE '%PUBLISH%' ORDER BY 2,1");
            cmd("DSPOBJD OBJ(QUSRSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/UOBJ)");
            q("QUSRSYS objects about the directory", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.UOBJ WHERE ODOBNM LIKE 'QGLD%' OR ODOBNM LIKE 'QDIR%' OR UPPER(ODOBTX) LIKE '%LDAP%' OR UPPER(ODOBTX) LIKE '%DIRECTORY%'");
            cmd("DSPOBJD OBJ(QSYS/*ALL) OBJTYPE(*CMD) OUTPUT(*OUTFILE) OUTFILE(QTEMP/SCMD)");
            q("QSYS commands with DIR or LDAP", "SELECT ODOBNM, ODOBTX FROM QTEMP.SCMD WHERE ODOBNM LIKE '%DIR%' OR ODOBNM LIKE '%LDAP%' OR ODOBNM LIKE '%GLD%' OR ODOBNM LIKE '%PUB%' ORDER BY 1");
            q("QSYS libraries with GLD", "SELECT ODOBNM, ODOBTX FROM QTEMP.SOBJ WHERE ODOBTP='*LIB' AND (ODOBNM LIKE '%GLD%' OR ODOBNM LIKE '%DIR%')");
        } else {
            q("QSYS objects about the directory (7.5 service)", "SELECT OBJNAME, OBJTYPE, OBJATTRIBUTE, OBJTEXT FROM TABLE(QSYS2.OBJECT_STATISTICS('QSYS', '*ALL', '*ALLSIMPLE')) X WHERE OBJNAME LIKE 'QGLD%' OR OBJNAME LIKE 'QDIR%' OR OBJNAME LIKE '%LDAP%'");
        }
        q("software products (Directory)", "SELECT PRODUCT_ID, PRODUCT_OPTION, RELEASE_LEVEL, TEXT_DESCRIPTION FROM QSYS2.SOFTWARE_PRODUCT_INFO WHERE UPPER(TEXT_DESCRIPTION) LIKE '%DIRECTORY%' OR UPPER(TEXT_DESCRIPTION) LIKE '%LDAP%'");
        q("QSYSINC members", "SELECT TABLE_NAME, SYSTEM_TABLE_MEMBER FROM QSYS2.SYSPARTITIONSTAT WHERE TABLE_SCHEMA='QSYSINC' AND (SYSTEM_TABLE_MEMBER LIKE '%LDAP%' OR SYSTEM_TABLE_MEMBER LIKE 'QGLD%' OR SYSTEM_TABLE_MEMBER LIKE 'LBER%' OR SYSTEM_TABLE_MEMBER LIKE '%DIR%') ORDER BY 1,2");
        q("QSYS2 routines", "SELECT ROUTINE_SCHEMA, ROUTINE_NAME, ROUTINE_TYPE FROM QSYS2.SYSROUTINES WHERE ROUTINE_NAME LIKE '%LDAP%' OR ROUTINE_NAME LIKE '%DIRECTORY%' OR ROUTINE_NAME LIKE '%DIRSRV%' ORDER BY 1,2");
        q("QSYS2 views", "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA IN ('QSYS2','SYSTOOLS','SYSIBMADM') AND (TABLE_NAME LIKE '%LDAP%' OR TABLE_NAME LIKE '%DIRECTORY%' OR TABLE_NAME LIKE '%DIRSRV%')");
        q("exit points", "SELECT EXIT_POINT_NAME, EXIT_POINT_FORMAT FROM QSYS2.EXIT_POINT_INFO WHERE EXIT_POINT_NAME LIKE '%GLD%' OR EXIT_POINT_NAME LIKE '%LDAP%' OR EXIT_POINT_NAME LIKE '%DIRSRV%'");
        q("function usage", "SELECT FUNCTION_ID, FUNCTION_NAME FROM QSYS2.FUNCTION_INFO WHERE FUNCTION_ID LIKE '%GLD%' OR FUNCTION_ID LIKE '%LDAP%' OR FUNCTION_ID LIKE '%DIR%'");
        q("tcpip servers", "SELECT * FROM QSYS2.TCPIP_SERVER_INFO WHERE SERVER_NAME LIKE '%DIR%' OR SERVER_NAME LIKE '%LDAP%'");
        q("service program exports (QGLD*)", "SELECT PROGRAM_LIBRARY, PROGRAM_NAME, SYMBOL_NAME, SYMBOL_USAGE FROM QSYS2.PROGRAM_EXPORT_IMPORT_INFO WHERE PROGRAM_LIBRARY='QSYS' AND PROGRAM_NAME LIKE 'QGLD%' ORDER BY 2,3");
        q("message files QGLD*", "SELECT MESSAGE_FILE_LIBRARY, MESSAGE_FILE, COUNT(*) FROM QSYS2.MESSAGE_FILE_DATA WHERE MESSAGE_FILE_LIBRARY='QSYS' AND MESSAGE_FILE LIKE 'QGLD%' GROUP BY MESSAGE_FILE_LIBRARY, MESSAGE_FILE");
        q("GLD message IDs", "SELECT MESSAGE_FILE, MESSAGE_ID, SEVERITY FROM QSYS2.MESSAGE_FILE_DATA WHERE MESSAGE_FILE_LIBRARY='QSYS' AND (MESSAGE_FILE LIKE 'QGLD%' OR MESSAGE_ID LIKE 'GLD%') ORDER BY 1,2");
        q("CPF messages about the directory server", "SELECT MESSAGE_FILE, MESSAGE_ID, SEVERITY FROM QSYS2.MESSAGE_FILE_DATA WHERE MESSAGE_FILE_LIBRARY='QSYS' AND MESSAGE_FILE='QCPFMSG' AND (UPPER(MESSAGE_TEXT) LIKE '%DIRECTORY SERVER%' OR UPPER(MESSAGE_TEXT) LIKE '%LDAP%' OR UPPER(MESSAGE_TEXT) LIKE '%DIRSRV%') ORDER BY 2");
        q("DCM application IDs (7.5)", "SELECT * FROM QSYS2.CERTIFICATE_USAGE_INFO WHERE UPPER(APPLICATION_ID) LIKE '%GLD%' OR UPPER(APPLICATION_ID) LIKE '%DIR%'");

        System.out.println("== commands (XML in OUTDIR)");
        for (String n : new String[] {"STRTCPSVR", "ENDTCPSVR", "CHGTCPSVR", "CHGDIRSVRA", "WRKDIRSVRA", "DSPDIRSVRA", "CFGDIRSVR", "CHGLDAPA", "WRKLDAP", "STRDIRSVR", "ENDDIRSVR", "QGLDPUBE", "CHGSYSDIRA", "WRKDIRE", "CFGTCP", "ADDTCPSVR", "QSYS/QGLDCFG"}) {
            try {
                Command cm = new Command(s, "/QSYS.LIB/" + n + ".CMD");
                String xml = cm.getXML();
                Files.writeString(out.resolve(n + ".xml"), xml);
                System.out.println("   " + n + " ok len=" + xml.length() + " cpp=" + cm.getCommandProcessingProgram());
            } catch (Exception e) { System.out.println("   " + n + " ERR " + e.getMessage()); }
        }

        System.out.println("== message files through JTOpen (IDs and severity; text in OUTDIR)");
        String hex = "0123456789ABCDEF";
        for (String mfn : new String[] {"QGLDMSG", "QGLDCMSG", "QGLDSMSG"}) {
            MessageFile mf = new MessageFile(s, "/QSYS.LIB/" + mfn + ".MSGF");
            int found = 0;
            try (PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve(mfn + "-messages.txt")))) {
                for (int d = 0; d < 10; d++) {
                    int miss = 0;
                    for (int n = 0; n < 4096 && miss < 768; n++) {
                        String id = "GLD" + d + hex.charAt(n >> 8) + hex.charAt((n >> 4) & 15) + hex.charAt(n & 15);
                        try {
                            AS400Message m = mf.getMessage(id);
                            System.out.println("   " + mfn + " " + id + " sev=" + m.getSeverity());
                            pw.println(id + " sev=" + m.getSeverity() + " | " + m.getText());
                            found++;
                            miss = 0;
                        } catch (Exception e) {
                            miss++;
                            if (d == 0 && n == 0 && e.getMessage() != null && e.getMessage().contains("CPF2407")) { d = 10; break; }
                        }
                    }
                }
            } catch (Exception e) { System.out.println("   " + mfn + " ERR " + e); }
            System.out.println("   " + mfn + " found=" + found);
        }

        System.out.println("== files");
        for (String p : new String[] {"/QIBM/UserData/OS400/DirSrv", "/QIBM/ProdData/OS400/DirSrv", "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIRDB", "/QIBM/UserData/OS400/DirSrv/ldap.conf", "/QIBM/ProdData/OS400/DirSrv/etc", "/QIBM/ProdData/OS400/DirSrv/bin", "/QIBM/ProdData/OS400/DirSrv/schema", "/QIBM/UserData/OS400/DirSrv/schema", "/QIBM/ProdData/OS400/DirSrv/samples", "/QIBM/ProdData/OS400/DirSrv/examples", "/etc/ldap.conf", "/QOpenSys/etc/ldap.conf", "/usr/bin/ldapsearch", "/QIBM/ProdData/OS400/QShell/bin"}) file(s, p, false);
        for (String p : new String[] {"/QIBM/ProdData/OS400/DirSrv/ldap.conf", "/QIBM/UserData/OS400/DirSrv/ldap.conf"}) file(s, p, true);

        System.out.println("== libraries QUSRDIRDB, QUSRDIRCL, QUSRDIRCF");
        q("directory db libs", "SELECT SCHEMA_NAME, SYSTEM_SCHEMA_NAME FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME LIKE 'QUSRDIR%' OR SCHEMA_NAME LIKE 'QGLD%'");
        q("directory db tables", "SELECT TABLE_SCHEMA, TABLE_NAME, SYSTEM_TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA LIKE 'QUSRDIR%' ORDER BY 1,2");

        System.out.println("== jobs QDIRSRV*, QGLD*");
        for (String pat : new String[] {"QDIRSRV*", "QGLD*", "QSYDIR*"}) {
            try {
                JobList jl = new JobList(s);
                jl.addJobSelectionCriteria(JobList.SELECTION_JOB_NAME, pat);
                Enumeration<?> en = jl.getJobs();
                while (en.hasMoreElements()) { Job j = (Job) en.nextElement(); System.out.println("   " + j.getNumber() + "/" + j.getUser() + "/" + j.getName() + " " + j.getStatus() + " " + j.getSubsystem() + " type=" + j.getType() + j.getSubtype()); }
            } catch (Exception e) { System.out.println("   ERR " + e); }
        }
        s.disconnectAllServices();
        c.close();
    }
}
