// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;

/**
 * Password level probing: start every service the oracle answers on, so each
 * server's seed exchange and start-server reply are recorded, and report the
 * password level the sign-on server announced.
 */
public final class PwdLevel {
    public static void main(String[] a) {
        AS400 signon = Hs.signonSystem();
        Hs.step("connect signon", () -> signon.connectService(AS400.SIGNON));
        Hs.step("signon password level", () -> Hs.out("     level=" + level(signon)));
        signon.disconnectAllServices();
        // Each server's seed exchange reply carries the level too; with the sign-on
        // server skipped JTOpen takes it from there.
        int[] services = {AS400.CENTRAL, AS400.COMMAND, AS400.DATABASE};
        String[] names = {"central", "command", "database"};
        for (int i = 0; i < services.length; i++) {
            int svc = services[i];
            AS400 s = Hs.system();
            Hs.step("connect " + names[i], () -> s.connectService(svc));
            Hs.step(names[i] + " password level", () -> Hs.out("     level=" + level(s)));
            s.disconnectAllServices();
        }
    }

    /** The level JTOpen learned at sign-on (a private field, read for the record). */
    static Object level(AS400 s) throws Exception {
        java.lang.reflect.Method m = AS400.class.getDeclaredMethod("getImpl");
        m.setAccessible(true);
        Object impl = m.invoke(s);
        for (Class<?> c = impl.getClass(); c != null; c = c.getSuperclass()) {
            for (java.lang.reflect.Field f : c.getDeclaredFields()) {
                if (f.getName().toLowerCase().contains("passwordlevel") || f.getName().equals("passwordType_")) {
                    f.setAccessible(true);
                    return f.getName() + ":" + f.get(impl);
                }
            }
        }
        return "unknown";
    }
}
