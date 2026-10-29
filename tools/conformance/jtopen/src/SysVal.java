// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.SystemValue;

/**
 * System values through JTOpen's SystemValue (program calls on the remote command
 * server). It needs the release and job CCSID from the sign-on and central servers,
 * so it fails where those servers do not answer.
 */
public final class SysVal {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.COMMAND));
        String[] names = {"QSECURITY", "QPWDLVL", "QCCSID", "QLANGID", "QCNTRYID", "QMAXSIGN", "QAUTOVRT"};
        for (String n : names) {
            Hs.step("get " + n, () -> {
                SystemValue v = new SystemValue(s, n);
                Hs.out("     " + n + "=" + v.getValue());
            });
        }
        s.disconnectAllServices();
    }
}
