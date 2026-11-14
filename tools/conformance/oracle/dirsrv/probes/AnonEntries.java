// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// Anonymous * + reads of cn=localhost, cn=ibmpolicies (subtrees) and cn=monitor (base).
// The host from QS_ORACLE_HOST; nothing is bound or written. Output: anon-entries.ldif.
import java.util.*;
import javax.naming.*;
import javax.naming.directory.*;
public final class AnonEntries {
  public static void main(String[] a) throws Exception {
    Hashtable<String,String> env = new Hashtable<>();
    env.put(Context.INITIAL_CONTEXT_FACTORY, "com.sun.jndi.ldap.LdapCtxFactory");
    env.put(Context.PROVIDER_URL, "ldap://" + System.getenv("QS_ORACLE_HOST") + ":389");
    env.put(Context.SECURITY_AUTHENTICATION, "none");
    env.put(Context.REFERRAL, "throw");
    env.put("java.naming.ldap.attributes.binary", "");
    String[][] q = {{"cn=localhost","2"},{"cn=ibmpolicies","2"},{"cn=monitor","0"}};
    for (String[] x : q) {
      try {
        DirContext ctx = new InitialDirContext(env);
        SearchControls sc = new SearchControls();
        sc.setSearchScope(Integer.parseInt(x[1]));
        sc.setReturningAttributes(new String[]{"*","+"});
        NamingEnumeration<SearchResult> en = ctx.search(x[0], "(objectClass=*)", sc);
        while (en.hasMore()) {
          SearchResult r = en.next();
          System.out.println("dn: " + r.getNameInNamespace());
          NamingEnumeration<? extends Attribute> ae = r.getAttributes().getAll();
          while (ae.hasMore()) { Attribute t = ae.next(); for (int i=0;i<t.size();i++) System.out.println(t.getID()+": "+t.get(i)); }
          System.out.println();
        }
        ctx.close();
      } catch (Exception e) { System.out.println("# " + x[0] + " ERR " + e); }
    }
  }
}
