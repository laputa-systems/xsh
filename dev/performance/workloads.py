from dataclasses import dataclass
from pathlib import Path

from bench.consolidation.fixtures import PACKAGE_ROW_SHAPES


SOURCE_ROOT = Path(__file__).resolve().parents[2]
STREAM_ITEMS = (65536, 131072, 262144)


@dataclass(frozen=True)
class Workload:
    name: str
    family: str
    shape: str
    script: Path
    arguments: tuple[str, ...]
    expected_stdout: bytes
    expected_subprocesses: int

    @property
    def useful_stdout_prefix(self):
        return self.expected_stdout.splitlines(keepends=True)[0]

    def argv(self, binary):
        return [str(binary), str(self.script), *(['--', *self.arguments] if self.arguments else [])]


def frozen_workloads(fixtures):
    cases = [
        Workload('startup-trivial', 'startup', 'common', SOURCE_ROOT / 'bench/stdlib-port/cold_trivial.xsh', (), b'hello\n', 0),
        Workload('startup-template', 'startup', 'demanding', SOURCE_ROOT / 'bench/stdlib-port/cold_template.xsh', (), b'hello xsh\n', 0),
    ]
    for shape, count in zip(('n', '2n', '4n'), STREAM_ITEMS):
        for mode in ('count', 'map-where'):
            cases.append(Workload(f'stream-{mode}-{shape}', 'streaming', shape, SOURCE_ROOT / 'bench/stream-stage-cost.xsh', (mode, str(count)), f'value={count}\n'.encode(), 0))
    for shape, count in zip(('n', '2n', '4n'), PACKAGE_ROW_SHAPES):
        root = fixtures / str(count)
        for script, family in (('json', 'algorithms'), ('directory', 'filesystem-process')):
            cases.append(Workload(f'{script}-{shape}', family, shape, SOURCE_ROOT / f'bench/consolidation/xsh/{script}.xsh', (str(root),), (root / 'expected' / script).read_bytes(), 0))
    cases.append(Workload('spawn-128', 'filesystem-process', 'common', SOURCE_ROOT / 'bench/consolidation/xsh/spawn.xsh', (), b''.join(f'probe {index}\n'.encode() for index in range(128)), 128))
    return tuple(cases)
