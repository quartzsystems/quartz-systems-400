# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# nslookup: header lines, answer forms per type, reverse lookups, errors,
# interactive commands. Only read-only queries through the system's resolver.

probe default 'nslookup localhost'
probe name 'nslookup pub400.com'
probe www 'nslookup www.pub400.com'
probe mx 'nslookup -type=mx pub400.com'
probe ns 'nslookup -query=ns pub400.com'
probe soa 'nslookup -type=soa pub400.com'
probe txt 'nslookup -q=txt pub400.com'
probe any 'nslookup -type=any pub400.com'
probe aaaa 'nslookup -type=aaaa pub400.com'
probe reverse 'nslookup 127.0.0.1'
probe reverse-public 'nslookup 185.113.5.134'
probe nxdomain 'nslookup nosuch.invalid; echo $?'
probe no-answer 'nslookup -type=mx localhost; echo $?'
probe server 'nslookup pub400.com 127.0.0.1; echo $?'
probe bad-type 'nslookup -type=bogus pub400.com; echo $?'
probe bad-option 'nslookup -frob pub400.com; echo $?'
probe interactive 'printf '\''pub400.com\nset type=mx\npub400.com\nset all\nexit\n'\'' | nslookup'
probe interactive-eof 'printf '\''localhost\n'\'' | nslookup; echo $?'
finish
