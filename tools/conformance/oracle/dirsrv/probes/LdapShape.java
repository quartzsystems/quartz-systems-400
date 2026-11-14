// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import javax.naming.*;
import javax.naming.directory.*;
import javax.naming.ldap.*;

/**
 * Read-only shape of the primary's directory: anonymous base and one-level
 * searches of each naming context (DN shapes, object classes, attribute
 * names; values only to OUTDIR), the StartTLS answer, the config directory
 * listing, the QSH LDAP tools and the server's jobs. Args: OUTDIR.
 */
public final class LdapShape {
    static String host, user, pass;
    static Path out;

    static Hashtable<String, String> env() {
        Hashtable<String, String> env = new Hashtable<>();
        env.put(Context.INITIAL_CONTEXT_FACTORY, "com.sun.jndi.ldap.LdapCtxFactory");
        env.put(Context.PROVIDER_URL, "ldap://" + host + ":389");
        env.put(Context.SECURITY_AUTHENTICATION, "none");
        env.put("com.sun.jndi.ldap.connect.timeout", "5000");
        env.put(Context.REFERRAL, "throw");
        return env;
    }

    static void search(String base, int scope, String filter, String[] attrs, PrintWriter pw) {
        System.out.println("== ldap base='" + base + "' scope=" + scope + " filter=" + filter);
        try {
            LdapContext ctx = new InitialLdapContext(env(), null);
            SearchControls sc = new SearchControls();
            sc.setSearchScope(scope);
            sc.setReturningAttributes(attrs);
            sc.setCountLimit(40);
            NamingEnumeration<SearchResult> en = ctx.search(base, filter, sc);
            int n = 0;
            try {
                while (en.hasMore()) {
                    SearchResult r = en.next();
                    String dn = r.getNameInNamespace();
                    pw.println("dn: " + dn);
                    StringBuilder names = new StringBuilder();
                    StringBuilder ocs = new StringBuilder();
                    NamingEnumeration<? extends Attribute> ae = r.getAttributes().getAll();
                    while (ae.hasMore()) {
                        Attribute a = ae.next();
                        names.append(a.getID()).append(' ');
                        for (int i = 0; i < a.size(); i++) {
                            pw.println(a.getID() + ": " + a.get(i));
                            if (a.getID().equalsIgnoreCase("objectclass")) ocs.append(a.get(i)).append(' ');
                        }
                    }
                    pw.println();
                    // DN shape: attribute types of each RDN, values masked.
                    String shape = dn.replaceAll("=([^,]*)", "=*");
                    System.out.println("   " + shape + "  oc[" + ocs.toString().trim() + "] attrs[" + names.toString().trim() + "]");
                    n++;
                }
            } catch (NamingException e) { System.out.println("   ERR(enum) " + e.getClass().getSimpleName() + " " + e.getExplanation()); }
            System.out.println("   entries=" + n);
            ctx.close();
        } catch (Exception e) { System.out.println("   ERR " + e.getClass().getSimpleName() + " " + e.getMessage()); }
    }

    static void list(AS400 s, String path) {
        try {
            IFSFile f = new IFSFile(s, path);
            if (!f.exists()) { System.out.println("   " + path + " absent"); return; }
            System.out.println("   " + path + " len=" + f.length() + " ccsid=" + f.getCCSID() + " dir=" + f.isDirectory());
            if (f.isDirectory()) { String[] l = f.list(); if (l != null) for (String n : l) System.out.println("      " + n); }
        } catch (Exception e) { System.out.println("   " + path + " ERR " + e); }
    }

    public static void main(String[] a) throws Exception {
        out = Paths.get(a[0]);
        Files.createDirectories(out);
        host = System.getenv("QS_ORACLE_HOST");
        user = System.getenv("QS_ORACLE_USER");
        pass = System.getenv("QS_ORACLE_PASS");

        // The naming contexts from the root DSE.
        List<String> ncs = new ArrayList<>();
        DirContext root = new InitialDirContext(env());
        Attribute nc = root.getAttributes("", new String[] {"namingcontexts"}).get("namingcontexts");
        for (int i = 0; i < nc.size(); i++) ncs.add((String) nc.get(i));
        root.close();
        ncs.add("cn=changelog");
        ncs.add("cn=configuration");
        ncs.add("cn=monitor");
        ncs.add("cn=pwdpolicy,cn=ibmpolicies");

        try (PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve("shape.ldif")))) {
            for (String b : ncs) {
                if (b.equalsIgnoreCase("cn=schema")) continue;
                search(b, SearchControls.OBJECT_SCOPE, "(objectClass=*)", new String[] {"*", "+"}, pw);
                search(b, SearchControls.ONELEVEL_SCOPE, "(objectClass=*)", new String[] {"objectclass"}, pw);
            }
            String sys = ncs.stream().filter(x -> x.toUpperCase().startsWith("OS400-SYS=")).findFirst().orElse(null);
            if (sys != null) {
                search("cn=accounts," + sys, SearchControls.ONELEVEL_SCOPE, "(objectClass=*)", new String[] {"*", "+"}, pw);
                search(sys, SearchControls.SUBTREE_SCOPE, "(os400-profile=QSECOFR)", new String[] {"*", "+"}, pw);
                search(sys, SearchControls.SUBTREE_SCOPE, "(objectclass=*)", new String[] {"objectclass"}, pw);
            }
        }

        // StartTLS with no certificate assigned: the result code.
        System.out.println("== StartTLS");
        try {
            LdapContext ctx = new InitialLdapContext(env(), null);
            StartTlsResponse tls = (StartTlsResponse) ctx.extendedOperation(new StartTlsRequest());
            System.out.println("   response " + tls.getID());
            try { tls.negotiate(); System.out.println("   negotiated"); tls.close(); } catch (Exception e) { System.out.println("   negotiate ERR " + e); }
            ctx.close();
        } catch (Exception e) { System.out.println("   ERR " + e.getClass().getSimpleName() + " " + e.getMessage()); }

        // WhoAmI anonymously (RFC 4532).
        System.out.println("== whoami");
        try {
            LdapContext ctx = new InitialLdapContext(env(), null);
            ExtendedResponse r = ctx.extendedOperation(new ExtendedRequest() {
                public String getID() { return "1.3.6.1.4.1.4203.1.11.3"; }
                public byte[] getEncodedValue() { return null; }
                public ExtendedResponse createExtendedResponse(String id, byte[] b, int o, int l) { return new ExtendedResponse() { public String getID() { return id; } public byte[] getEncodedValue() { return b == null ? null : Arrays.copyOfRange(b, o, o + l); } }; }
            });
            System.out.println("   ok " + (r.getEncodedValue() == null ? "-" : new String(r.getEncodedValue())));
            ctx.close();
        } catch (Exception e) { System.out.println("   ERR " + e.getClass().getSimpleName() + " " + e.getMessage()); }

        AS400 s = new AS400(host, user, pass);
        System.out.println("== files");
        for (String p : new String[] {"/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR", "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc", "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/logs", "/QIBM/UserData/OS400/DirSrv/idsinstinfo", "/QIBM/ProdData/OS400/DirSrv/etc/SchemaV6.2", "/QIBM/ProdData/OS400/DirSrv/UserTools", "/usr/bin", "/QIBM/ProdData/OS400/QShell"}) list(s, p);
        System.out.println("== jobs of user QDIRSRV");
        try {
            JobList jl = new JobList(s);
            jl.addJobSelectionCriteria(JobList.SELECTION_USER_NAME, "QDIRSRV");
            Enumeration<?> en = jl.getJobs();
            while (en.hasMoreElements()) { Job j = (Job) en.nextElement(); System.out.println("   " + j.getNumber() + "/" + j.getUser() + "/" + j.getName() + " " + j.getStatus() + " " + j.getSubsystem() + " type=" + j.getType() + j.getSubtype() + " jobd=" + j.getValue(Job.JOB_DESCRIPTION)); }
        } catch (Exception e) { System.out.println("   ERR " + e); }
        System.out.println("== QDIRSRV profile");
        try { User u = new User(s, "QDIRSRV"); System.out.println("   status=" + u.getStatus() + " class=" + u.getUserClassName() + " uid=" + u.getUserID() + " special=" + String.join(",", u.getSpecialAuthority())); } catch (Exception e) { System.out.println("   ERR " + e); }
        s.disconnectAllServices();
    }
}
