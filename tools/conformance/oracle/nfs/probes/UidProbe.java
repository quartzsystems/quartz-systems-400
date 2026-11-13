// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.util.*;

/**
 * Read-only: the UID, GID and status of every profile whose name begins
 * with Q (the shipped ones) on the primary. Prints NAME UID GID STATUS.
 */
public final class UidProbe {
    public static void main(String[] a) throws Exception {
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"),
                System.getenv("QS_ORACLE_PASS"));
        UserList l = new UserList(s);
        l.setUserInfo(UserList.USER);
        Enumeration<?> e = l.getUsers();
        List<String> out = new ArrayList<>();
        while (e.hasMoreElements()) {
            User u = (User) e.nextElement();
            String n = u.getName();
            if (!n.startsWith("Q")) continue;
            u.loadUserInformation();
            out.add(String.format("%-10s %10d %10d %s", n, u.getUserID(), u.getGroupID(), u.getStatus()));
        }
        Collections.sort(out);
        out.forEach(System.out::println);
        s.disconnectAllServices();
    }
}
