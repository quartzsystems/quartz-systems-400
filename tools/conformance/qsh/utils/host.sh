# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# host: one line a record, default types, -t, reverse, -v/-a, errors.

probe default 'host pub400.com'
probe localhost 'host localhost'
probe mx 'host -t mx pub400.com'
probe ns 'host -t ns pub400.com'
probe soa 'host -t soa pub400.com'
probe txt 'host -t txt pub400.com'
probe none 'host -t aaaa localhost; echo $?'
probe reverse 'host 127.0.0.1'
probe nxdomain 'host nosuch.invalid; echo $?'
probe verbose 'host -v pub400.com'
probe all 'host -a pub400.com'
probe server 'host pub400.com 127.0.0.1'
probe bad-type 'host -t bogus pub400.com; echo $?'
probe usage 'host; echo $?'
finish
