// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.SpooledFileList;

/** Network print server: list the user's spooled files (read only). */
public final class Splf {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.PRINT));
        Hs.step("list", () -> {
            SpooledFileList l = new SpooledFileList(s);
            l.setUserFilter("*CURRENT");
            l.openSynchronously();
            Hs.out("     count=" + l.size());
            l.close();
        });
        s.disconnectAllServices();
    }
}
