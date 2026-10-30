# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# dig: the banner, header and sections, +short, +noall +answer, -x, @server,
# statuses. Read-only queries.

probe default 'dig pub400.com'
probe short 'dig +short pub400.com'
probe short-mx 'dig +short pub400.com mx'
probe noall-answer 'dig +noall +answer pub400.com'
probe reverse 'dig -x 127.0.0.1 +short'
probe nxdomain 'dig nosuch.invalid; echo $?'
probe root 'dig +noall +answer | head -3'
probe server 'dig @127.0.0.1 pub400.com +short'
probe nocmd 'dig +nocmd +nostats pub400.com'
probe bad-plus 'dig +frob pub400.com; echo $?'
probe bad-type 'dig -t bogus pub400.com; echo $?'
probe help 'dig -h | head -3; echo $?'
finish
