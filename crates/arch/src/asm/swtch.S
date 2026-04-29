/*
 * Context switch.
 *
 *   void swtch(struct context **old, struct context *new);
 *
 * Save the current register context in *old and load the register
 * context from new. Callee-saved registers only — the caller saves
 * the rest per the System V AMD64 ABI.
 *
 * Arguments:
 *   %rdi : struct context **old
 *   %rsi : struct context  *new
 *
 * Mirrors xv6-64's swtch.S. AT&T syntax (set explicitly because Rust's
 * global_asm! default is Intel).
 */

.text
.globl swtch
swtch:
        /* Save old callee-saved registers on the current stack. */
        pushq   %rbp
        pushq   %rbx
        pushq   %r11
        pushq   %r12
        pushq   %r13
        pushq   %r14
        pushq   %r15

        /* Switch stacks: *old = %rsp; %rsp = new. */
        movq    %rsp, (%rdi)
        movq    %rsi, %rsp

        /* Load new callee-saved registers from the new stack. */
        popq    %r15
        popq    %r14
        popq    %r13
        popq    %r12
        popq    %r11
        popq    %rbx
        popq    %rbp

        ret
