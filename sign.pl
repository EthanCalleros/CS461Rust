#!/usr/bin/env perl
open(SIG, $ARGV[0]) || die "open $ARGV[0]: $!";
my $buf; read(SIG, $buf, 1000); close(SIG);
if(length($buf) > 510) { print STDERR "boot block too large: " . length($buf) . " bytes (max 510)\n"; exit 1; }
$buf .= "\0" x (510 - length($buf));
$buf .= "\x55\xAA";
open(SIG, ">$ARGV[0]") || die "open >$ARGV[0]: $!";
print SIG $buf; close(SIG);
