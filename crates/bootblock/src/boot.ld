SECTIONS
{
  . = 0x7C00;
  .text : {
    *(.text.bootasm)
    *(.text .text.*)
  }
}
