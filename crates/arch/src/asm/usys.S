# Userspace syscall stubs.
# Expanded from the C-preprocessor macro version since global_asm!
# does not run cpp. Each stub: load syscall number into %rax,
# move %rcx to %r10 (syscall clobbers rcx), execute syscall, ret.

/* Syscall numbers (from syscall.h) */
.set SYS_fork,    1
.set SYS_exit,    2
.set SYS_wait,    3
.set SYS_pipe,    4
.set SYS_read,    5
.set SYS_kill,    6
.set SYS_exec,    7
.set SYS_fstat,   8
.set SYS_chdir,   9
.set SYS_dup,    10
.set SYS_getpid, 11
.set SYS_sbrk,  12
.set SYS_sleep,  13
.set SYS_uptime, 14
.set SYS_open,   15
.set SYS_write,  16
.set SYS_mknod,  17
.set SYS_unlink, 18
.set SYS_link,   19
.set SYS_mkdir,  20
.set SYS_close,  21

.text

.global fork
fork:
    mov $SYS_fork, %rax
    mov %rcx, %r10
    syscall
    ret

.global exit
exit:
    mov $SYS_exit, %rax
    mov %rcx, %r10
    syscall
    ret

.global wait
wait:
    mov $SYS_wait, %rax
    mov %rcx, %r10
    syscall
    ret

.global pipe
pipe:
    mov $SYS_pipe, %rax
    mov %rcx, %r10
    syscall
    ret

.global read
read:
    mov $SYS_read, %rax
    mov %rcx, %r10
    syscall
    ret

.global write
write:
    mov $SYS_write, %rax
    mov %rcx, %r10
    syscall
    ret

.global close
close:
    mov $SYS_close, %rax
    mov %rcx, %r10
    syscall
    ret

.global kill
kill:
    mov $SYS_kill, %rax
    mov %rcx, %r10
    syscall
    ret

.global exec
exec:
    mov $SYS_exec, %rax
    mov %rcx, %r10
    syscall
    ret

.global open
open:
    mov $SYS_open, %rax
    mov %rcx, %r10
    syscall
    ret

.global mknod
mknod:
    mov $SYS_mknod, %rax
    mov %rcx, %r10
    syscall
    ret

.global unlink
unlink:
    mov $SYS_unlink, %rax
    mov %rcx, %r10
    syscall
    ret

.global fstat
fstat:
    mov $SYS_fstat, %rax
    mov %rcx, %r10
    syscall
    ret

.global link
link:
    mov $SYS_link, %rax
    mov %rcx, %r10
    syscall
    ret

.global mkdir
mkdir:
    mov $SYS_mkdir, %rax
    mov %rcx, %r10
    syscall
    ret

.global chdir
chdir:
    mov $SYS_chdir, %rax
    mov %rcx, %r10
    syscall
    ret

.global dup
dup:
    mov $SYS_dup, %rax
    mov %rcx, %r10
    syscall
    ret

.global getpid
getpid:
    mov $SYS_getpid, %rax
    mov %rcx, %r10
    syscall
    ret

.global sbrk
sbrk:
    mov $SYS_sbrk, %rax
    mov %rcx, %r10
    syscall
    ret

.global sleep
sleep:
    mov $SYS_sleep, %rax
    mov %rcx, %r10
    syscall
    ret

.global uptime
uptime:
    mov $SYS_uptime, %rax
    mov %rcx, %r10
    syscall
    ret
