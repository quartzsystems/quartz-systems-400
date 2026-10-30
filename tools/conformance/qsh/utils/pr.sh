# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# pr: header line and date form, page length, trailer, -t, -d, -n, -o, columns, -m, -F, -e, -i, messages.

probe default 'pr data/fruits.txt | od -c | head -20'
probe count 'pr data/fruits.txt | wc -l'
probe header 'pr data/fruits.txt | sed -n 3p; pr -h '\''Title X'\'' data/fruits.txt | sed -n 3p'
probe stdin-header 'pr < data/fruits.txt | sed -n 3p'
probe length 'pr -l 13 data/nums.txt'
probe length-small 'pr -l 8 data/nums.txt'
probe t 'pr -t data/fruits.txt'
probe double 'pr -t -d data/fruits.txt | od -c'
probe number 'pr -t -n data/fruits.txt; pr -t -n:3 data/fruits.txt'
probe offset 'pr -t -o 3 data/fruits.txt'
probe columns 'i=1; while [ $i -le 7 ]; do echo line$i; i=$((i+1)); done > f; pr -t -3 f; pr -t -3 -a f; pr -t -2 -w 20 f'
probe separator 'i=1; while [ $i -le 4 ]; do echo line$i; i=$((i+1)); done > f; pr -t -2 -s, f; pr -t -2 -s f | od -c'
probe merge 'pr -t -m data/fruits.txt data/nums.txt'
probe formfeed 'pr -F data/fruits.txt | od -c | tail -4'
probe expand 'pr -t -e4 data/tabs.txt; printf '\''a         b\n'\'' | pr -t -i | od -c'
probe page 'i=1; while [ $i -le 120 ]; do echo l$i; i=$((i+1)); done > f; pr +2 f | grep Page; pr +9 f; echo $?'
probe several 'pr data/fruits.txt data/csv.txt | grep Page'
probe missing 'pr data/nosuch data/fruits.txt | grep Page; pr -r data/nosuch'
probe empty 'pr data/empty.txt | wc -l'
probe bad 'pr -z data/fruits.txt; pr -l x data/fruits.txt; pr -m -2 data/fruits.txt'
finish
