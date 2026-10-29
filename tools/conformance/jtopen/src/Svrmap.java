// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;

/** Port mapper: every service looked up through as-svrmap (449), then connected. */
public final class Svrmap {
    public static void main(String[] a) {
        AS400 s = Hs.system(false);
        int[] services = {AS400.SIGNON, AS400.CENTRAL, AS400.COMMAND, AS400.DATABASE,
            AS400.DATAQUEUE, AS400.FILE, AS400.PRINT};
        String[] names = {"signon", "central", "command", "database", "dataqueue", "file", "print"};
        for (int i = 0; i < services.length; i++) {
            int svc = services[i];
            Hs.step("connect " + names[i], () -> s.connectService(svc));
        }
        s.disconnectAllServices();
    }
}
