import argparse
import os
from pathlib import Path

from dev.performance.workloads import SOURCE_ROOT, Workload


_SHAPES = {
    'small': (64 * 1024, 256, 32, 2),
    'medium': (1024 * 1024, 2048, 256, 8),
    'large': (16 * 1024 * 1024, 16384, 2048, 32),
}
_FAMILIES = ('wc', 'sort', 'awk', 'traversal', 'pipeline', 'spawn')


def _parts(name):
    parts = name.split('-')
    if len(parts) != 3 or parts[0] != 'core' or parts[1] not in _FAMILIES or parts[2] not in _SHAPES:
        raise ValueError('unknown core workload')
    return parts[1], parts[2]


def _root(fixtures, name):
    _parts(name)
    if not fixtures.is_dir() or fixtures != fixtures.resolve():
        raise ValueError('core fixture parent must be an existing nonsymlink directory')
    root = fixtures / name
    if root.is_symlink():
        raise ValueError('core fixture root must not be a symlink')
    return root


def _inputs(name):
    family, shape = _parts(name)
    byte_count, records, files, pipelines = _SHAPES[shape]
    if family == 'wc':
        block = b'alpha beta\n' + b'\x00\xffz\n' + b'x' * 49 + b'\n'
        return {'input': (block * (byte_count // len(block) + 1))[:byte_count]}, {}
    if family in ('sort', 'awk'):
        rows = [f'key-{index:06}\t{index * 17 % records}\t{index % 13}\n'.encode() for index in reversed(range(records))]
        return {'input': b''.join(rows)}, {}
    if family == 'traversal':
        contents = {f'tree/{".hidden" if index // 16 % 2 else "group"}-{index // 16:04}/file {index:06}.txt':
                    f'payload {index}\n'.encode() for index in range(files)}
        contents['tree/.gitignore'] = b'*.txt\n'
        return contents, {'tree/link-file': 'group-0000/file 000000.txt',
                          'tree/link-parent': '.', 'tree/link-dangling': 'missing'}
    return {}, {}


def _directories(contents, links):
    directories = set()
    for name in (*contents, *links):
        directories.update(parent.as_posix() for parent in Path(name).parents if parent != Path('.'))
    return directories


def describe_case(name, *, fixtures, source_root):
    root = _root(fixtures, name)
    family, shape = _parts(name)
    byte_count, records, files, pipelines = _SHAPES[shape]
    contents, links = _inputs(name)
    script_root = source_root / 'dev/performance/fixtures/core-cases'
    if family == 'wc':
        expected = f'{contents["input"].count(bytes([10]))} {root / "input"}\n'.encode()
        return Workload(name, 'byte-stream', shape, source_root / 'core/wc.xsh',
                        ('--lines', str(root / 'input')), expected, 0)
    if family == 'sort':
        rows = contents['input'].splitlines(keepends=True)
        expected = b''.join(sorted(rows, key=lambda row: int(row.split(b'\t')[1])))
        return Workload(name, 'records-ordering', shape, source_root / 'core/sort.xsh',
                        ('-t', '\t', '-k2,2n', str(root / 'input')), expected, 0)
    if family == 'awk':
        program = '{ total += $2; groups[$3]++ } END { print NR, total, groups[0], groups[12] }'
        expected = f'{records} {records * (records - 1) // 2} {(records + 12) // 13} {records // 13}\n'.encode()
        return Workload(name, 'records-awk', shape, source_root / 'core/awk.xsh',
                        (program, str(root / 'input')), expected, 0)
    if family == 'traversal':
        rows = {path.removeprefix('tree/'): f'file\t{len(data)}' for path, data in contents.items()}
        rows.update({path.removeprefix('tree/'): 'dir\t-' for path in _directories(contents, links) if path != 'tree'})
        rows.update({path.removeprefix('tree/'): 'link\t-' for path in links})
        expected = ''.join(f'{path}\t{rows[path]}\n' for path in sorted(rows)).encode()
        return Workload(name, 'filesystem-process', shape, script_root / 'traversal.xsh',
                        (str(root / 'tree'),), expected, 0)
    if family == 'spawn':
        return Workload(name, 'filesystem-process', shape, script_root / 'spawn.xsh',
                        (str(pipelines),), f'{pipelines}\n'.encode(), pipelines)
    expected = b''.join(f'probe {index}\n'.encode() for index in range(pipelines))
    return Workload(name, 'filesystem-process', shape, script_root / 'pipeline.xsh',
                    (str(pipelines),), expected, pipelines * 2)


def prepare_case(name, *, fixtures, source_root=SOURCE_ROOT):
    root = _root(fixtures, name)
    contents, links = _inputs(name)
    root.mkdir(exist_ok=False)
    for directory in sorted(_directories(contents, links)):
        (root / directory).mkdir()
    for relative, data in contents.items():
        (root / relative).write_bytes(data)
    for relative, target in links.items():
        (root / relative).symlink_to(target)
    return describe_case(name, fixtures=fixtures, source_root=source_root)


def validate_case(case, *, fixtures, source_root=SOURCE_ROOT):
    root = _root(fixtures, case.name)
    if not root.is_dir() or case != describe_case(case.name, fixtures=fixtures, source_root=source_root):
        raise ValueError('core workload descriptor or fixture root changed')
    contents, links = _inputs(case.name)
    expected_directories = _directories(contents, links)
    actual_directories, actual_files, actual_links = set(), set(), set()
    for directory, dirs, files in os.walk(root, followlinks=False):
        for name in dirs + files:
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                actual_links.add(relative)
                if relative not in links or os.readlink(path) != links[relative]:
                    raise ValueError('core fixture link changed')
            elif path.is_dir():
                actual_directories.add(relative)
            elif path.is_file():
                actual_files.add(relative)
                if relative not in contents or path.read_bytes() != contents[relative]:
                    raise ValueError('core fixture file changed')
            else:
                raise ValueError('unsupported core fixture entry')
    if actual_directories != expected_directories or actual_files != contents.keys() or actual_links != links.keys():
        raise ValueError('core fixture entries changed')


def work_counts(case):
    family, shape = _parts(case.name)
    byte_count, records, files, pipelines = _SHAPES[shape]
    if family == 'wc':
        return {'input_bytes': byte_count}
    if family in ('sort', 'awk'):
        return {'input_records': records}
    if family == 'traversal':
        contents, links = _inputs(case.name)
        return {'completed_entries': len(contents) + len(links) + len(_directories(contents, links)) - 1,
                'regular_files': files + 1, 'regular_file_bytes': sum(map(len, contents.values()))}
    if family == 'spawn':
        return {'completed_children': pipelines, 'subprocess_starts': pipelines}
    return {'completed_pipelines': pipelines, 'subprocess_starts': pipelines * 2,
            'completed_records': pipelines}


def snapshot(root):
    case = describe_case(root.name, fixtures=root.parent, source_root=SOURCE_ROOT)
    validate_case(case, fixtures=root.parent)
    return {'case': case.name, 'work': work_counts(case)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixtures', required=True, type=Path)
    parser.add_argument('--case', required=True)
    args = parser.parse_args()
    prepare_case(args.case, fixtures=args.fixtures)
