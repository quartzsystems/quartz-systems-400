// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Message;

/** What every program of the suite shares: the system, result lines, step guards. */
final class Hs {
    private Hs() {}

    /** The host (the recording proxy: 127.0.0.1). */
    static String host() {
        return env("QS_HS_HOST");
    }

    /** The user. */
    static String user() {
        return env("QS_HS_USER");
    }

    /** The password. */
    static String pass() {
        return env("QS_HS_PASS");
    }

    private static String env(String k) {
        String v = System.getenv(k);
        if (v == null || v.isEmpty()) {
            throw new IllegalStateException(k + " is not set (run through tn5250-driver hostsvr-trace)");
        }
        return v;
    }

    /** A system on the well-known ports (no port mapper). */
    static AS400 system() {
        return system(true);
    }

    /** A system; `defaultPorts` false sends every lookup to the port mapper. */
    static AS400 system(boolean defaultPorts) {
        return system(defaultPorts, 0);
    }

    /** A system with a fixed job CCSID (0: whatever the central server says). */
    static AS400 system(boolean defaultPorts, int ccsid) {
        AS400 s = new AS400(host(), user(), pass());
        if (ccsid != 0) {
            try {
                s.setCcsid(ccsid);
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        }
        try {
            s.setGuiAvailable(false);
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
        if (defaultPorts) {
            s.setServicePortsToDefault();
        }
        if (skipSignon()) {
            try {
                java.lang.reflect.Field f = AS400.class.getDeclaredField("skipSignonServer_");
                f.setAccessible(true);
                f.setBoolean(s, true);
                // The implementation only skips the sign-on server once it believes the
                // password level is known; each server's seed reply then supplies it.
                java.lang.reflect.Method choose = AS400.class.getDeclaredMethod("chooseImpl");
                choose.setAccessible(true);
                choose.invoke(s);
                java.lang.reflect.Method get = AS400.class.getDeclaredMethod("getImpl");
                get.setAccessible(true);
                Object impl = get.invoke(s);
                java.lang.reflect.Field t = impl.getClass().getDeclaredField("isPasswordTypeSet_");
                t.setAccessible(true);
                t.setBoolean(impl, true);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
        return s;
    }

    /**
     * Whether to start each server directly (seed exchange and start server on its own
     * port) instead of signing on through the sign-on server first: QS_HS_SKIP_SIGNON=1.
     */
    static boolean skipSignon() {
        return "1".equals(System.getenv("QS_HS_SKIP_SIGNON"));
    }

    /** A system that always goes through the sign-on server. */
    static AS400 signonSystem() {
        AS400 s = new AS400(host(), user(), pass());
        try {
            s.setGuiAvailable(false);
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
        s.setServicePortsToDefault();
        return s;
    }

    /** A result line. */
    static void out(String line) {
        System.out.println(line);
    }

    /** A step that may throw. */
    interface Step {
        void run() throws Exception;
    }

    /** Run a step; a failure is a result line, not the end of the program. */
    static void step(String name, Step s) {
        try {
            s.run();
            out("ok   " + name);
        } catch (Exception e) {
            out("fail " + name + ": " + describe(e));
        }
    }

    /** An exception as a line: its class and message, with return codes where JTOpen has them. */
    static String describe(Throwable e) {
        while (e instanceof java.lang.reflect.InvocationTargetException && e.getCause() != null) {
            e = e.getCause();
        }
        StringBuilder b = new StringBuilder(e.getClass().getSimpleName());
        if (e instanceof com.ibm.as400.access.AS400SecurityException se) {
            b.append(" rc=").append(se.getReturnCode());
        } else if (e instanceof com.ibm.as400.access.ServerStartupException ss) {
            b.append(" rc=").append(ss.getReturnCode());
        } else if (e instanceof java.sql.SQLException q) {
            b.append(" state=").append(q.getSQLState()).append(" code=").append(q.getErrorCode());
        } else if (e instanceof com.ibm.as400.access.AS400Exception ae) {
            AS400Message[] m = ae.getAS400MessageList();
            if (m != null) {
                for (AS400Message x : m) {
                    b.append(' ').append(x.getID());
                }
            }
        }
        String msg = e.getMessage();
        if (msg != null) {
            b.append(": ").append(msg.replace('\n', ' ').replace(user(), "{user}"));
        }
        return b.toString();
    }

    /** Messages as result lines. */
    static void messages(String what, AS400Message[] list) {
        if (list == null) {
            return;
        }
        for (AS400Message m : list) {
            out("     " + what + " " + m.getID() + " type=" + m.getType() + " sev=" + m.getSeverity()
                + " text=" + String.valueOf(m.getText()).replace(user(), "{user}"));
        }
    }
}
