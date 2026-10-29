# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# xargs: grouping, -I, -L, -n, -s, -E, -t, quoting, statuses, messages.

probe default 'printf "a b\nc\n" | xargs'
probe utility 'printf "a\nb\n" | xargs echo got'
probe n 'printf "1 2 3 4 5\n" | xargs -n 2 echo'
probe L 'printf "a b\nc\nd e f\n" | xargs -L 2 echo'
probe I 'printf "one\n  two words\n" | xargs -I {} echo "[{}]"'
probe I-twice 'echo x | xargs -I % echo % and %'
probe quotes 'printf "\"a b\" c\n" | xargs -n 1 echo'
probe backslash 'printf "s\\\\ t\n" | xargs -n 1 echo'
probe unterminated 'printf "\"abc\n" | xargs echo'
probe trace 'echo a | xargs -t echo'
probe eof 'printf "a b STOP c\n" | xargs -E STOP echo'
probe size 'printf "aaaa bbbb cccc\n" | xargs -s 15 echo'
probe too-long 'printf "aaaaaaaaaaaaaaaaaaaa\n" | xargs -s 10 echo'
probe no-input 'printf "" | xargs echo none; echo $?'
probe missing 'echo a | xargs nosuchcmd'
probe status-123 'echo a | xargs false; echo $?'
probe status-255 'echo a | xargs sh -c "exit 255"; echo $?'
probe bad-n 'echo a | xargs -n 0 echo'
probe find 'find data/sub -type f | xargs wc -l'
probe script 'printf "echo script got \$1\n" > s.sh; chmod +x s.sh; echo arg | xargs ./s.sh'
finish
