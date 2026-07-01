#!/usr/bin/env perl
use strict;
use warnings;
use Time::HiRes qw(time);
use File::Spec;

my $baseline = $ENV{XSH_PGO_BENCH_BASELINE} // 'target/bench-release-base/release/xsh';
my $pgo = $ENV{XSH_PGO_BENCH_PGO} // 'target/bench-release-pgo/release/xsh';
my $corpus = File::Spec->rel2abs($ENV{XSH_PGO_BENCH_CORPUS} // 'target/pgo-bench/corpus');
my $scale = $ENV{XSH_PROF_SCALE} // 64;
my $batches = $ENV{XSH_PGO_BENCH_BATCHES} // 5;
my $rep_scale = $ENV{XSH_PGO_BENCH_REP_SCALE} // 1;
my $out = $ENV{XSH_PGO_BENCH_OUT} // '/tmp/xsh-pgo-bench.csv';

sub scaled_reps {
  my ($base) = @_;
  my $value = int($base * $rep_scale);
  return $value < 1 ? 1 : $value;
}

sub run_cmd {
  my (@cmd) = @_;
  system(@cmd) == 0 or die "command failed: @cmd\n";
}

sub run_quiet {
  my (@cmd) = @_;
  open(my $oldout, '>&', STDOUT) or die "dup stdout: $!\n";
  open(my $olderr, '>&', STDERR) or die "dup stderr: $!\n";
  open(STDOUT, '>', '/dev/null') or die "redirect stdout: $!\n";
  open(STDERR, '>', '/dev/null') or die "redirect stderr: $!\n";
  my $ok = system(@cmd) == 0;
  open(STDOUT, '>&', $oldout) or die "restore stdout: $!\n";
  open(STDERR, '>&', $olderr) or die "restore stderr: $!\n";
  die "command failed: @cmd\n" unless $ok;
}

sub median {
  my @values = sort { $a <=> $b } @_;
  return $values[int(@values / 2)];
}

sub run_workload_once {
  my ($bin, $commands) = @_;

  for my $args (@$commands) {
    run_quiet($bin, @$args);
  }
}

sub measure {
  my ($bin, $reps, $commands) = @_;
  my @times;

  run_workload_once($bin, $commands);

  for my $batch (1 .. $batches) {
    my $start = time();
    for my $i (1 .. $reps) {
      run_workload_once($bin, $commands);
    }
    push @times, time() - $start;
  }

  return median(@times);
}

my @workloads = (
  ['scenario', 'archive-package', 50, [['perf/scenarios/archive-package.xsh', '--', $corpus]]],
  ['scenario', 'extension-count', 50, [['perf/scenarios/extension-count.xsh', '--', $corpus]]],
  ['scenario', 'json-log-rollup', 50, [['perf/scenarios/json-log-rollup.xsh', '--', $corpus]]],
  ['scenario', 'manifest-hash', 50, [['perf/scenarios/manifest-hash.xsh', '--', $corpus]]],
  ['startup', 'startup-minimal', 200, [['perf/startup/minimal.xsh']]],
  ['startup', 'startup-control', 200, [['perf/startup/control-flow.xsh']]],
  ['startup', 'startup-modules', 200, [['perf/startup/modules.xsh']]],
  ['frontend', 'frontend-compound-types', 100, [['perf/frontend/compound-types.xsh']]],
  ['frontend', 'frontend-control-flow', 100, [['perf/frontend/control-flow-and-effects.xsh']]],
  ['frontend', 'frontend-pipeline-shapes', 100, [['perf/frontend/pipeline-shapes.xsh']]],
  ['interpreter', 'interp-json-record', 20, [['perf/interpreter/json-record-glue-1k.xsh']]],
  ['interpreter', 'interp-pipeline-sort', 20, [['perf/interpreter/pipeline-sort-ir-glue-5k.xsh']]],
  ['interpreter', 'interp-regex', 20, [['perf/interpreter/regex-ir-glue-5k.xsh']]],
  [
    'mixed',
    'mixed-cli',
    25,
    [
      ['perf/startup/minimal.xsh'],
      ['perf/frontend/control-flow-and-effects.xsh'],
      ['perf/interpreter/json-record-glue-1k.xsh'],
      ['perf/scenarios/extension-count.xsh', '--', $corpus],
      ['perf/interpreter/pipeline-sort-ir-glue-5k.xsh'],
      ['perf/scenarios/manifest-hash.xsh', '--', $corpus],
    ],
  ],
);

run_cmd('mkdir', '-p', 'target/pgo-bench');
run_cmd($baseline, 'perf/make-corpus.xsh', '--', '--root', $corpus, '--scale', $scale);

open(my $fh, '>', $out) or die "open $out: $!\n";
print $fh "group,workload,reps,baseline_s,pgo_s,delta_pct\n";

for my $workload (@workloads) {
  my ($group, $name, $base_reps, $commands) = @$workload;
  my $reps = scaled_reps($base_reps);
  my $base_s = measure($baseline, $reps, $commands);
  my $pgo_s = measure($pgo, $reps, $commands);
  my $delta = (($pgo_s - $base_s) / $base_s) * 100.0;
  printf $fh "%s,%s,%d,%.6f,%.6f,%+.2f\n", $group, $name, $reps, $base_s, $pgo_s, $delta;
  printf "%s %-28s reps=%d baseline=%.3fs pgo=%.3fs delta=%+.2f%%\n", $group, $name, $reps, $base_s, $pgo_s, $delta;
}

print "wrote $out\n";
