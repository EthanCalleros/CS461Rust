# Initial process execs /init.

/* Syscall numbers (inlined from syscall.h) */
.set SYS_exec, 7
.set SYS_exit, 2

# exec(init, argv)
.code64
.global start
start:
  mov $init, %rdi
  mov $argv, %rsi
  mov $SYS_exec, %rax
  syscall

# for(;;) exit();
exit:
  mov $SYS_exit, %rax
  syscall
  jmp exit

# char init[] = "/init\0";
init:
  .string "/init\0"

# char *argv[] = { init, 0 };
.p2align 3
argv:
  .quad init
  .quad 0