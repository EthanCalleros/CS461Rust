use crate::lib::{outb, inb}; // Assuming these are in your lib.rs or arch crate
use core::ptr;

const COM1: u16 = 0x3f8;

static mut UART_EXISTS: bool = false;

/// Early initialization: sets up the 8250 UART to 9600 baud.
/// Called before memory management or interrupts are fully up.
pub unsafe fn uartearlyinit() {
    // 1. Turn off the FIFO
    outb(COM1 + 2, 0);

    // 2. Set baud rate (9600)
    // Access DLAB (Divisor Latch Access Bit) to set baud rate
    outb(COM1 + 3, 0x80); 
    outb(COM1 + 0, (115200 / 9600) as u8); // Low byte
    outb(COM1 + 1, 0);                     // High byte
    
    // 3. 8 data bits, 1 stop bit, no parity, lock divisor
    outb(COM1 + 3, 0x03);
    
    // 4. Modem control: Disable hangup
    outb(COM1 + 4, 0);
    
    // 5. Enable receive interrupts
    outb(COM1 + 1, 0x01);

    // If status is 0xFF, the hardware isn't responding (no serial port)
    if inb(COM1 + 5) == 0xFF {
        return;
    }

    UART_EXISTS = true;

    // Announce boot
    for &c in b"xv6-rust...\n" {
        uartputc(c as i32);
    }
}

/// Post-boot initialization: enables IRQs via the IOAPIC.
pub unsafe fn uartinit() {
    if !UART_EXISTS {
        return;
    }

    // Clear any pending interrupts by reading registers
    inb(COM1 + 2);
    inb(COM1 + 0);
    
    // Assuming you have an ioapic crate/module
    // ioapic::enable(IRQ_COM1, 0); 
}

/// Put a single character out of the serial port.
/// Spins until the Transmit Holding Register is empty.
pub unsafe fn uartputc(c: i32) {
    if !UART_EXISTS {
        return;
    }

    // Wait for the UART to be ready to transmit (Line Status Register bit 5)
    // We limit the retry count so we don't hang the whole kernel if hardware fails.
    for _ in 0..128 {
        if (inb(COM1 + 5) & 0x20) != 0 {
            break;
        }
        // Optional: microdelay(10); if you've implemented timer delays
    }
    
    outb(COM1 + 0, c as u8);
}

/// Internal helper to grab a character if available.
unsafe fn uartgetc() -> i32 {
    if !UART_EXISTS {
        return -1;
    }
    
    // Check if Data Ready (Line Status Register bit 0)
    if (inb(COM1 + 5) & 0x01) == 0 {
        return -1;
    }

    inb(COM1 + 0) as i32
}

/// The interrupt handler called by trap.c when COM1 fires.
pub unsafe fn uartintr() {
    // console::intr needs a function pointer to uartgetc.
    // In Rust, we can pass a wrapper or the function itself.
    extern "C" {
        fn consoleintr(getc: unsafe fn() -> i32);
    }
    consoleintr(uartgetc);
}
