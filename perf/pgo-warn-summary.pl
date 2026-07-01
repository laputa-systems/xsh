#!/usr/bin/env perl
use strict;
use warnings;

my %counts;
my $total = 0;

while (my $line = <>) {
  next unless $line =~ /warning:.*profile/i || $line =~ /-Wbackend-plugin/;
  $total++;

  my $bucket = 'unknown';
  if ($line =~ /\/([^\/\s]+)\.rs[:\]]/) {
    $bucket = $1;
  } elsif ($line =~ /crate `([^`]+)`/) {
    $bucket = $1;
  } elsif ($line =~ /function ([^\s]+)/) {
    $bucket = $1;
  }

  $counts{$bucket}++;
}

print "missing_profile_warnings,total,$total\n";
for my $bucket (sort { $counts{$b} <=> $counts{$a} || $a cmp $b } keys %counts) {
  print "missing_profile_warnings,$bucket,$counts{$bucket}\n";
}
