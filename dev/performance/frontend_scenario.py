import argparse
import base64
from pathlib import Path
import tempfile

from dev.performance import frontend_cases, inference_measurement, owned_case


class Sampler:
    def __init__(self, leases):
        self.leases = leases
        self.expected_stdout = None

    def invoke(self, argv, directory, environment, timeout):
        row = owned_case.run.invoke(
            argv, directory, environment, timeout,
            on_launch=lambda: self.leases.prepare_cleanup_unresolved('frontend diagnostic process launch'),
            on_cleanup_verified=self.leases.resolve_cleanup_unresolved,
        )
        if type(row['status']) is not int:
            raise ValueError('frontend diagnostic did not report a child status')
        if row['timed_out']:
            raise ValueError('frontend diagnostic timed out before completion')
        if row['status'] != 0:
            stderr = base64.b64decode(row['stderr_base64'], validate=True)
            raise owned_case.WorkloadFailure(f'frontend diagnostic failed: argv={argv!r} status={row["status"]} stderr={stderr[:4096]!r}')
        if row['effects'] != {} or base64.b64decode(row['stderr_base64'], validate=True):
            raise owned_case.WorkloadFailure('frontend diagnostic emitted errors or left effects')
        if self.expected_stdout is not None and base64.b64decode(row['stdout_base64'], validate=True) != self.expected_stdout:
            raise owned_case.WorkloadFailure('frontend diagnostic output differs from the oracle')
        return row


def check_frontend(xsh, xsht, scratch, leases):
    for product in (xsh, xsht):
        if not product.is_absolute() or product != product.resolve() or not product.is_file():
            raise ValueError('frontend products must be absolute regular files')
    if not scratch.is_absolute() or scratch != scratch.resolve() or not scratch.is_dir():
        raise ValueError('frontend scratch must be an existing absolute directory')
    with tempfile.TemporaryDirectory(prefix='frontend-scenario-', dir=scratch) as directory:
        work = Path(directory)
        sources = frontend_cases.prepare_cases('edit', root=work / 'sources')
        sampler = Sampler(leases)
        selected = frontend_cases.representatives('edit')
        for workload in selected:
            directories = sources[workload.name]
            frontend_cases.validate_sources(workload, directories)
            frontend, runtime, phases = {}, {}, {}
            for variant in inference_measurement.VARIANTS:
                source = directories[variant]
                entry = source / workload.entry
                environment = inference_measurement.environment_for(source)
                frontend[variant] = sampler.invoke([str(xsht), 'frontend-stats', '--json', str(entry)], source, environment, 60)
                runtime[variant] = sampler.invoke([str(xsh), str(entry)], source, environment, 60)
                if base64.b64decode(runtime[variant]['stdout_base64'], validate=True) != workload.expected_stdout:
                    raise owned_case.WorkloadFailure(f'{workload.name} {variant} runtime output differs from the oracle')
                sampler.expected_stdout = workload.expected_stdout
                phases[variant] = inference_measurement.collect_runtime_phase_diagnostic(
                    xsht, entry, work / f'{workload.name}-{variant}.json', sampler, environment, workload.expected_stdout)
                sampler.expected_stdout = None
            frontend_cases.validate_case_evidence(workload, directories=directories, frontend_rows=frontend,
                                                  runtime_rows=runtime, phase_diagnostics=phases)
        return len(selected)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--xsh', type=Path, required=True)
    parser.add_argument('--xsht', type=Path, required=True)
    parser.add_argument('--scratch', type=Path, required=True)
    args = parser.parse_args()
    return owned_case.execute('frontend diagnostic',
                              lambda leases: f'{check_frontend(args.xsh, args.xsht, args.scratch, leases)} workloads; disposal execution and scaling unqualified')


if __name__ == '__main__':
    raise SystemExit(main())
