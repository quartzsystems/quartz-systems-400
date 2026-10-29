// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;

/**
 * Central server: the job CCSID through NLS exchange attributes and
 * conversion table downloads (single-byte to and from Unicode). JTOpen keeps
 * these behind package-private entry points, reached here by reflection.
 */
public final class Central {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.CENTRAL));
        Object[] impl = {null};
        Hs.step("impl", () -> {
            java.lang.reflect.Method m = AS400.class.getDeclaredMethod("getImpl");
            m.setAccessible(true);
            impl[0] = m.invoke(s);
        });
        Hs.step("ccsid from server", () -> {
            java.lang.reflect.Method m = impl[0].getClass().getMethod("getCcsidFromServer");
            Hs.out("     ccsid=" + m.invoke(impl[0]));
        });
        int[][] tables = {{37, 13488, 1}, {13488, 37, 2}, {273, 13488, 1}, {290, 1200, 1}};
        for (int[] t : tables) {
            Hs.step("table " + t[0] + " to " + t[1], () -> download(impl[0], t[0], t[1], t[2]));
        }
        s.disconnectAllServices();
    }

    static void download(Object impl, int from, int to, int type) throws Exception {
        Class<?> c = Class.forName("com.ibm.as400.access.NLSTableDownload");
        Object d = c.getConstructor(impl.getClass()).newInstance(impl);
        c.getMethod("connect").invoke(d);
        try {
            char[] table = (char[]) c.getMethod("download", int.class, int.class, int.class).invoke(d, from, to, type);
            int sum = 0;
            if (table != null) {
                for (char ch : table) {
                    sum = sum * 31 + ch;
                }
            }
            Hs.out("     chars=" + (table == null ? -1 : table.length) + " hash=" + Integer.toHexString(sum));
        } finally {
            c.getMethod("disconnect").invoke(d);
        }
    }
}
