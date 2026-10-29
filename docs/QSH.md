# QSH: Qshell on QS/400

Qshell is written from scratch in Rust. No existing shell, coreutils or
shell-utilities code or crate is used or ported; behavior follows the
POSIX shell specification and Qshell's documented and observed
behavior. This document has two parts: the command language (this
section, crate `qsh/qsh`) and the utilities (added with the
`qsh-utils` crates).

## Language

### Crate layout

`qsh/qsh` (package `qsh`):

| Module | What |
|---|---|
| `syntax/` | the tree with spans (`ast.rs`), errors, the parser: `input.rs` (cursor, line continuation, alias splicing), `token.rs`, `word.rs` (quoting and expansions read straight into word parts), `command.rs` (grammar), `heredoc.rs` |
| `arith/` | `$((...))` and `let`: lexer, precedence-climbing parser, evaluator over signed 64-bit integers |
| `pattern/` | `*`, `?`, bracket expressions with classes; `path.rs` pathname expansion over a `Dirs` source |
| `expand/` | tilde, parameters (`param.rs`), command substitution, arithmetic, field splitting (`split.rs`), globbing, quote removal |
| `shell/` | the interpreter state (`mod.rs`), variables, options, descriptor table, jobs, traps; `exec/` lists, pipelines, simple and compound commands, redirections |
| `builtins/` | special and regular builtins |
| `sys/` | the `Sys` seam, `Stream`s, `errno`, the utility registry, `MemSys`, `HostSys` |
| `session/` | entry points: `qsh` arguments, `run`, `Interactive`, `batch` |
| `signal.rs`, `msg.rs` | signal table, diagnostics |

### Grammar

The full POSIX `sh` grammar as Qshell implements it:

- Words: single quotes, double quotes, backslash, line continuation
  (backslash-newline, removed everywhere except single quotes and
  quoted here-documents), comments at word start.
- Parameter expansion: `$name`, `${name}`, positional `$1`..`$9` and
  `${10}`, special `$? $$ $! $# $@ $* $- $0`; `${x-w} ${x:-w} ${x=w}
  ${x:=w} ${x?w} ${x:?w} ${x+w} ${x:+w} ${#x} ${x%w} ${x%%w} ${x#w}
  ${x##w}`. The word of the test forms is tilde-expanded and, unquoted,
  field-split.
- Command substitution `$(...)` (parsed in line with the outer text,
  so here-documents and `case` work inside) and backquotes (with their
  own backslash rule, parsed separately), nested to any depth up to the
  nesting limit.
- Arithmetic `$((...))`: decimal, `0` octal, `0x` hex and `base#digits`
  (2 to 64) constants; variables by name (a value that is itself an
  expression is evaluated, 32 levels deep at most); unary `+ - ! ~`,
  prefix and postfix `++ --`, `* / % + - << >> < <= > >= == != & ^ |
  && || ?:`, every assignment operator and `,`. `&&`, `||` and `?:`
  skip the side effects of the branch not taken. Overflow wraps.
  `$((` that does not close with `))` is a command substitution of a
  subshell.
- Tilde expansion at word start and after `=`/`:` in assignments.
- Field splitting by `IFS` (whitespace runs, non-whitespace
  delimiters, `""` adjacent to an expansion keeps an empty field),
  pathname expansion, quote removal.
- Simple commands with assignments and redirections `< > >> >| <> <&
  >& n> n<` and here-documents `<<` and `<<-` (expanded unless any part
  of the delimiter is quoted).
- Pipelines with `!`, and-or lists, `&`, `;`, newlines.
- Compound commands: `( )`, `{ }`, `if/elif/else`, `while`, `until`,
  `for` (with and without `in`), `case` (with `(` before patterns, `|`
  alternatives, last `;;` optional).
- Function definitions `name() compound-command [redirections]`.
- Aliases, expanded in command position before the parser sees the
  word; an alias ending in a blank expands the next word too; an alias
  is not re-expanded inside its own text. The shell parses one complete
  command at a time, so an alias defined on a line applies from the
  next line.

Every command node carries a `Span` (offsets and line); syntax errors
name the line. The parser never panics (a random-input test runs 25,000
inputs) and nesting deeper than 100 levels is an error, not a stack
overflow, on a 2 MB thread.

### Execution model

The shell keeps its own descriptor table (`fd number -> Fd`, an
`Arc<dyn Stream>`), working directory, umask, variables, functions,
aliases, options, traps and jobs. Everything else goes through `Sys`.

- A subshell (`( )`, command substitution, a script run because the
  file is not a program) is a copy of the shell state run on the same
  thread.
- Each element of a multi-command pipeline is a subshell started with
  `Sys::fork` (a thread for `MemSys` and `HostSys`) and joined by
  `Sys::pipe` pipes. A writer whose reader has gone gets `EPIPE`; a
  shell writing to such a pipe ends as `SIGPIPE` would, so `while :; do
  echo; done | head` ends.
- `&` lists are subshells started with `Sys::fork`; their standard input
  is `/dev/null` unless `set -m`. `$!` is the child's process ID.
- Programs are looked up as special builtin, function, regular builtin,
  then `PATH` (cached by `hash`) and run with `Sys::spawn`, which gets
  the argument list, the exported environment plus the command's
  assignments, the working directory, the umask and the descriptor map.
  `ENOEXEC` from `spawn` means "a file that is not a program": the
  shell runs it as a script in a new shell environment (exported
  variables only, no functions or aliases, fresh options).
- Traps run between commands: `Sys::take_signals(pid)` returns signals
  that arrived for the (sub)shell; a signal without a trap ends the
  shell with status 128+n unless its default is to ignore it; an
  interactive shell ignores `INT`, `TERM` and `QUIT`.
- With `set -m` on an interactive shell and a `Sys::job_control()`, a
  foreground program gets the terminal for its duration and a stopped
  one becomes a job.

### Builtins

Special builtins (assignments before them persist; their errors end a
non-interactive shell): `: . break continue eval exec exit export
readonly return set shift times trap unset`.

Regular builtins: `[ alias bg builtin cd command declare echo false fg
getopts hash jobs kill let local print printf pwd read source test true
type typeset ulimit umask unalias wait`.

Output formats (scripts parse these; POSIX forms unless noted):

| Command | Form |
|---|---|
| `set` | `name=value` per line, sorted, value quoted for re-input (`'...'`, `'\''` for a quote) |
| `export -p`, `readonly -p` | `export name=value`, `export name` when unset |
| `declare -p` | `declare -- name=value`, attributes as `-irx` |
| `set -o` | `name` padded to 16, then `on`/`off` |
| `set +o` | `set -o name` / `set +o name` |
| `trap` | `trap -- 'action' NAME` (`EXIT` for 0, names without `SIG`) |
| `alias` | `name='value'` |
| `jobs` | `[n] + State command` (`Running`, `Done`, `Done(n)`, `Stopped(SIGx)`); `-l` adds the process ID after the mark; `-p` process IDs only |
| `type`, `command -V` | `x is a shell builtin`, `x is a special shell builtin`, `x is a shell function`, `x is an alias for v`, `x is a shell keyword`, `x is /path` |
| `command -v` | name, path, or `alias x='v'` |
| `kill -l` | signal names on one line |
| `umask` | four octal digits; `-S` as `u=rwx,g=rx,o=rx` |
| `times` | `XmY.YYs XmY.YYs` twice |
| `ulimit -a` | one line per limit, label padded to 21 |
| `hash` | one path per line |

`echo` interprets `\a \b \c \f \n \r \t \v \\ \0nnn` and takes `-n`
(no newline). `print [-nrR] [-u n]` is ksh's. `printf` supports
`%s %b %c %d %i %o %u %x %X %e %E %f %F %g %G %%`, flags `-+ #0`, `*`
width and precision, `'c` character codes, and reuses the format until
the arguments are used. `read [-r] [-p prompt] [-u n]` reads a byte at
a time so nothing past the line is consumed. `local` keeps the outer
value until assigned; `declare`/`typeset` in a function make the name
local. `getopts` keeps its position inside clustered options between
calls; assigning `OPTIND` resets it.

### Variables

Set at start: the environment (exported), `IFS` (space, tab, newline),
`PATH` (`/usr/bin:.` when not in the environment), `PS1` (`$ `), `PS2`
(`> `), `PS4` (`+ `), `PWD`, `PPID`, `OPTIND`, `QSH_VERSION` (the
release, `V1R0M0`), `HOME` from the user's profile when the environment
has none. Dynamic: `RANDOM` (0 to 32767; assigning seeds it), `SECONDS`
(assigning sets the base), `LINENO`. `ENV` names a file an interactive
shell reads at start; `TRACEFILE` names the `set -o trace` file
(default `$HOME/qsh_trace`); `CDPATH` is searched by `cd`.

### Options

`set -abCefmntuvx`, `-o allexport errexit ignoreeof monitor noclobber
noexec noglob nolog notify nounset trace verbose xtrace`; `qsh` also
takes `-c`, `-i`, `-l` (login: `/etc/profile` and `$HOME/.profile`),
`-s`. `$-` lists the letters in the order `abCefimnstuvx`. `errexit`
does not apply to conditions, non-last members of and-or lists or
negated pipelines. `trace` appends each command's text to the trace
file. `nolog` is accepted (there is no history file).

### Signals

Signal names and numbers are the system's (`QSYSINC/SYS(SIGNAL)`), so
`kill -6` is `TERM` and `trap ... 4` is `INT`. Status for a command
ended by a signal is 128+n.

### Diagnostics

Every message is `qsh: <id> <text>` on standard error. The identifiers
are interface; the texts are Quartz-written.

| Id | When |
|---|---|
| 001-0002 | `cd` cannot enter a directory |
| 001-0014 | command not found (status 127) |
| 001-0019 | command found but not runnable (status 126) |
| 001-0021 | `set -u` and an unset parameter |
| 001-0022 | `${x?word}` |
| 001-0023 | a redirection cannot open its file |
| 001-0024 | a descriptor that is not open |
| 001-0029 | assigning a read-only variable |
| 001-0030 | a builtin failed for a system reason |
| 001-0040 | no such job |
| 001-0045 | not a signal |
| 001-0050 | syntax error (status 2) |
| 001-0051 | substitution not valid at run time |
| 001-0062 | builtin usage |
| 001-0063 | not a valid name |
| 001-0067 | arithmetic error |
| 001-0068 | not a number |

### The `Sys` seam

`qsh::sys::Sys` (Send + Sync, no per-shell state) is everything outside
the language:

- environment and identity: `environ`, `initial_cwd`, `pid`, `ppid`,
  `home_of`;
- files (absolute paths): `open(path, OpenFlags, mode) -> Fd`, `stat`,
  `lstat`, `read_dir`, `access`, `realpath`, `case_insensitive(dir)`,
  `chdir`, and for utilities `mkdir rmdir unlink rename link symlink
  readlink chmod chown utimes` (default `ENOSYS`);
- `pipe`;
- processes: `spawn(SpawnRequest) -> Box<dyn Child>` (a utility thread
  or a child job), `fork(name, body) -> Box<dyn Child>` (shell code
  run concurrently), `kill`, `take_signals(pid)`, `signal_action`,
  `job_control() -> Option<&dyn JobControl>`;
- time and limits: `now`, `times`, `get_limit`, `set_limit`.

`Stream` (`read`, `write`, `is_terminal`, `as_any`) is an open
description; `Fd = Arc<dyn Stream>` and the last drop closes it.
In-process kinds: pipes, `Bytes` (here-documents), `Buffer`
(collecting output), `Null`. Errors are `SysError { errno }` with the
system's `errno` numbers (`ENOEXEC`, which the system lacks, is 3600
and never leaves the shell).

Utilities implement `qsh::sys::Utility` (`run(&mut UtilCtx) -> i32`;
closures work) and are registered by path in a `Registry`. `UtilCtx`
carries the arguments, environment, working directory, umask,
descriptor map and `&dyn Sys`.

`MemSys` is the test system: an in-memory file system with the root's
naming rule (case-insensitive except under `/QOpenSys`), `/dev/null`,
registered utilities run on threads, recorded signals, a settable
clock and settable limits. `HostSys` maps the shell's `/` to a host
directory through `std`, runs registered utilities in-process and other
programs as host processes (copying non-file streams through threads).

### Entry points

- `session::Invocation::parse(argv)`, `Invocation::session()`,
  `Invocation::command(cmd)`.
- `session::shell_for(sys, &inv, stdin, stdout, stderr) -> Shell`:
  options, arguments, startup files.
- `session::run(sys, &inv, stdin, stdout, stderr) -> i32`: `-c`, a
  script or standard input, to the exit status after the `EXIT` trap.
- `session::Interactive::new(shell)`: `prompt()` (`PS1`/`PS2`
  expanded), `feed_line(line) -> Feed::{Ready, More, Exit(n)}`,
  `feed_eof()`, `notices()` (finished background jobs). The 5250 QSH
  session and the SSH TTY feed lines; a line ending in an odd number of
  backslashes continues.
- `session::batch::run_cmd(sys, cmd, stdin, job_stdout) ->
  BatchResult { status, message }` for `QSH CMD()`:
  `QIBM_QSH_CMD_OUTPUT` `STDOUT` (default, the job's stream), `NONE`,
  `FILE=path` (replace), `FILEAPPEND=path` (both standard output and
  error go there); the message is `QSH0005` (status, completion; escape
  when `QIBM_QSH_CMD_ESCAPE_MSG=Y` and the status is above 0),
  `QSH0006` (signal, escape) or `QSH0007` (exception, escape).
  `CmdOutput::parse` and `ending_message` are usable on their own.
- `Shell::run_text(src, line)`, `Shell::run_list`, `Shell::var`,
  `Shell::status`, `Shell::set_fd`.

### Tests

- `tests/corpus/*.sht`: 692 cases (quoting, parameters, every
  expansion operator, command substitution, arithmetic, splitting,
  globbing including `/QSYS.LIB` and `/QOpenSys` case rules, tilde,
  redirections, here-documents, pipelines, lists, compound commands,
  `case`, loops, functions, aliases, variables, options, `test`,
  `echo`/`print`/`printf`, `read`, `getopts`, traps and signals, jobs,
  `cd`/`pwd`/`umask`, `eval`/`.`/`exec`/`command`/`type`/`hash`,
  scripts) with expected stdout, stderr and status, run against
  `MemSys`. `QSH_UPDATE_CORPUS=1` rewrites expectations;
  `QSH_CORPUS_ONLY=text` runs matching cases.
- `tests/fuzz.rs`: random token soup and random bytes through the
  parser, deep nesting and recursion on a 2 MB stack, random safe
  scripts through the interpreter.
- `tests/session.rs`: the interactive session, startup files, `qsh`
  arguments, the batch path, `HostSys` on a temporary directory.

### Open questions

The oracle's sign-on server dropped every connection on 2026-10-24 and
the host timed out on 2026-10-29, so none of the following was settled
on the system. The probe scripts are
in `tools/conformance/qsh/lang/probes.txt` (run through `QSH CMD()`
with `QIBM_QSH_CMD_OUTPUT=FILE=` under `/home/QSTEST`, copied to a
`QSTEST` member with `CPYFRMSTMF` and read over JDBC).

1. Message identifiers: only 001-0014 (not found) and 001-0050 (syntax
   error) are believed to match Qshell; the others in the table are
   placeholders to replace with the system's numbers.
2. Whether `QIBM_QSH_CMD_OUTPUT=FILE=` also receives standard error
   (assumed yes).
3. `echo`: `-n` and escape handling as implemented; whether Qshell
   also takes `-e`.
4. Output formats of `set -o`, `ulimit -a`, `kill -l`, `hash`, `type`
   for a name not found (stdout or stderr, status), `times`.
5. Whether `printf`, `print`, `let`, `declare`, `typeset`, `local`,
   `builtin`, `source`, `fg`, `bg` are Qshell builtins, and whether
   `whence` exists.
6. `local x` without a value: keeps the outer value (ash) or unsets
   (ksh, bash).
7. The `trace` option's letter (`-t` assumed) and the trace file's
   format (command text only assumed; the system may add time stamps).
8. Exit status for a command ended by a signal (128+n assumed; ksh uses
   256+n).
9. `$-` letter order; whether `i` and `s` appear.
10. Syntax error line numbers at end of input (the line after the last
    newline is reported).
11. `$'...'`, `function name { }`, and `name()` with `-` in the name: not
    implemented; assumed absent from Qshell.
12. Whether the last element of a pipeline runs in the current shell
    (assumed not: `echo x | read v` leaves `v` unset).
13. Pathname expansion sort order (byte order assumed; the system may
    collate by CCSID).
14. Arithmetic width (64-bit assumed) and whether variables holding
    expressions are evaluated recursively.
