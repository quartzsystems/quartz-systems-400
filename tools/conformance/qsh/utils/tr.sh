# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# tr: ranges, classes, -c -d -s, repeats, escapes, messages.

probe range 'tr a-z A-Z < data/fruits.txt'
probe classes 'tr "[:lower:]" "[:upper:]" < data/fruits.txt'
probe delete 'tr -d aeiou < data/fruits.txt'
probe squeeze 'echo "aaa  bbb   ccc" | tr -s " "; echo aabbcc | tr -s abc xyz'
probe complement 'echo a1b2c3 | tr -c "0-9\n" "#"; echo a1b2c3 | tr -cd 0-9; echo'
probe delete-squeeze 'echo aabbccdd | tr -ds a b'
probe short-second 'echo abcdef | tr a-f xy'
probe escapes 'printf "a b\n" | tr " " "\n"; echo abc | tr b "\101"'
probe repeat 'echo abcdef | tr a-f "[x*]"; echo abcdef | tr a-f "[x*2]yz"'
probe backwards 'echo x | tr z-a A'
probe usage 'tr; echo x | tr -d a b'
probe bad-class 'echo x | tr "[:nope:]" a'
probe equivalence 'echo banana | tr "[=a=]" o'
probe repeat-first 'echo x | tr "[a*]" b'
finish
