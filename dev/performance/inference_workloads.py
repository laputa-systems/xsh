from dataclasses import dataclass
from types import MappingProxyType
from typing import Mapping


SCALING_SIZES = (64, 128, 256, 512)
RECORD_DEPTH_LIMIT = 256
FAMILIES = (
    'alias-chain',
    'dependency-chain',
    'records',
    'higher-order-composition',
    'generic-stream',
    'recursive-effect-component',
    'module-diamond',
    'finite-overload-chain',
    'named-generic-constructors',
)


EXPECTED_STDOUT = {
    'named-generic-constructors': {shape: {size: output for size in SCALING_SIZES} for shape, output in (('independent', b'7\n'), ('shared-default', b'9\n'), ('ground-carrier', b'7\n'), ('phantom-context', b'7\n'))},
    'alias-chain': {'chain': {64: b'42\n', 128: b'42\n', 256: b'42\n', 512: b'42\n'}},
    'dependency-chain': {'chain': {64: b'64\n', 128: b'128\n', 256: b'256\n', 512: b'512\n'}},
    'records': {
        'wide': {64: b'7\n', 128: b'7\n', 256: b'7\n', 512: b'7\n'},
        'deep': {64: b'7\n', 128: b'7\n', 256: b'7\n', 512: b'7\n'},
    },
    'higher-order-composition': {'chain': {64: b'64\n', 128: b'128\n', 256: b'256\n', 512: b'512\n'}},
    'generic-stream': {'items': {64: b'64\n64\n', 128: b'128\n128\n', 256: b'256\n256\n', 512: b'512\n512\n'}},
    'recursive-effect-component': {'component': {64: b'42\n', 128: b'42\n', 256: b'42\n', 512: b'42\n'}},
    'module-diamond': {'diamonds': {64: b'128\n', 128: b'256\n', 256: b'512\n', 512: b'1024\n'}},
    'finite-overload-chain': {'chain': {64: b'1\n', 128: b'1\n', 256: b'1\n', 512: b'1\n'}},
}


OUTPUT_SHAPES = {
    'named-generic-constructors': {'independent': 'size independently parameterized Box fields', 'shared-default': 'size Box fields with shared input and checked defaults', 'ground-carrier': 'size Branded fields carrying a closed Brand', 'phantom-context': 'Int payload after size context-anchored Phantom constructors'},
    'alias-chain': {'chain': 'Int'},
    'dependency-chain': {'chain': 'Int'},
    'records': {'wide': 'Int field projection', 'deep': 'Int leaf projection'},
    'higher-order-composition': {'chain': 'Int'},
    'generic-stream': {'items': 'List[Int] for inputs 7 and 9'},
    'recursive-effect-component': {'component': 'Int'},
    'module-diamond': {'diamonds': 'Int sum'},
    'finite-overload-chain': {'chain': 'List[Int]'},
}


EVIDENCE_SHAPES = {
    'named-generic-constructors': {'independent': 'size independent generic parameters and size Box constructor instances', 'shared-default': 'one shared generic parameter and size defaulted Box constructor instances', 'ground-carrier': 'one generic parameter and size Branded instances with ground return schema carriers', 'phantom-context': 'one generic payload parameter and size context-anchored Phantom[Int] instances'},
    'alias-chain': {'chain': 'size callable aliases and one Int specialization'},
    'dependency-chain': {'chain': 'size direct function dependencies'},
    'records': {
        'wide': 'one projection from a size-field record',
        'deep': 'one projection through a size-level record shape',
    },
    'higher-order-composition': {'chain': 'size applications of one Int callback contract'},
    'generic-stream': {'items': 'one Int item type at two inputs and size yielded items each'},
    'recursive-effect-component': {'component': 'one size-function recursive component with a time edge'},
    'module-diamond': {'diamonds': 'size two-branch diamonds sharing one identity export'},
    'finite-overload-chain': {'chain': 'size selected List[Int] map overloads'},
}


@dataclass(frozen=True)
class InferenceWorkload:
    name: str
    family: str
    shape: str
    size: int
    entry: str
    annotated_sources: Mapping[str, str]
    reduced_sources: Mapping[str, str]
    expected_units: int
    logical_output_shape: str
    logical_evidence_shape: str
    expected_stdout: bytes
    frontend_expectation: str

    @property
    def expected_runtime_bytes(self):
        return len(self.expected_stdout)


def _workload(family, shape, size, annotated, reduced, *, frontend_expectation='unprobed'):
    name = f'{family}-{shape}-{size}'
    expected_stdout = EXPECTED_STDOUT[family][shape][size]
    return InferenceWorkload(
        name=name,
        family=family,
        shape=shape,
        size=size,
        entry='main.xsh',
        annotated_sources=MappingProxyType(dict(annotated)),
        reduced_sources=MappingProxyType(dict(reduced)),
        expected_units=size,
        logical_output_shape=OUTPUT_SHAPES[family][shape],
        logical_evidence_shape=EVIDENCE_SHAPES[family][shape],
        expected_stdout=expected_stdout,
        frontend_expectation=frontend_expectation,
    )


def _alias_chain(size):
    def source(annotated):
        lines = ['pure identity(value) { value }', '']
        lines.extend(f'let alias_{index} = ' + ('identity' if index == 0 else f'alias_{index - 1}')
                     for index in range(size))
        result_type = ': Int' if annotated else ''
        lines.extend(('', f'let answer{result_type} = alias_{size - 1}(42)', 'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('alias-chain', 'chain', size, source(True), source(False))


def _dependency_chain(size):
    def source(annotated):
        lines = []
        for index in range(size):
            argument_type = ': Int' if annotated else ''
            return_type = ' -> Int' if annotated else ''
            body = 'value + 1' if index == 0 else f'step_{index - 1}(value) + 1'
            lines.append(f'pure step_{index}(value{argument_type}){return_type} {{ {body} }}')
        result_type = ': Int' if annotated else ''
        lines.extend(('', f'let answer{result_type} = step_{size - 1}(0)', 'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('dependency-chain', 'chain', size, source(True), source(False))


def _wide_record(size):
    fields = ', '.join(f'field_{index}: Int' for index in range(size))
    values = ', '.join(f'field_{index}: {7 if index == 0 else 1}' for index in range(size))

    def source(annotated):
        lines = []
        if annotated:
            lines.extend((f'type Wide = {{{fields}}}', ''))
            lines.append('pure project(value: Wide) -> Int { value.field_0 }')
        else:
            lines.append('pure project(value) { value.field_0 }')
        lines.extend(('', f'let answer = project({{{values}}})', 'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('records', 'wide', size, source(True), source(False))


def _deep_record(size):
    def source(annotated):
        lines = ['type Deep_0 = {leaf: Int}']
        lines.extend(f'type Deep_{index} = {{next: Deep_{index - 1}}}' for index in range(1, size + 1))
        lines.extend(('', 'pure make_0(value: Int) -> Deep_0 { {leaf: value} }' if annotated
                      else 'pure make_0(value) { {leaf: value} }'))
        for index in range(1, size + 1):
            if annotated:
                lines.append(f'pure make_{index}(value: Int) -> Deep_{index} {{ {{next: make_{index - 1}(value)}} }}')
            else:
                lines.append(f'pure make_{index}(value) {{ {{next: make_{index - 1}(value)}} }}')
        lines.extend(('', 'pure project_0(value: Deep_0) -> Int { value.leaf }' if annotated
                      else 'pure project_0(value) { value.leaf }'))
        for index in range(1, size + 1):
            if annotated:
                lines.append(f'pure project_{index}(value: Deep_{index}) -> Int {{ project_{index - 1}(value.next) }}')
            else:
                lines.append(f'pure project_{index}(value) {{ project_{index - 1}(value.next) }}')
        if annotated:
            answer_annotation = ': Int'
        else:
            answer_annotation = ''
        lines.extend(('', f'let answer{answer_annotation} = project_{size}(make_{size}(7))', 'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    expectation = 'expected-type-depth-rejection' if size > RECORD_DEPTH_LIMIT else 'boundary-pending'
    return _workload('records', 'deep', size, source(True), source(False), frontend_expectation=expectation)


def _higher_order_composition(size):
    def source(annotated):
        lines = []
        if annotated:
            lines.extend(('type Unary = pure(value: Int) -> Int',
                          'pure increment(value: Int) -> Int { value + 1 }', ''))
        else:
            lines.extend(('pure increment(value) { value + 1 }', ''))
        for index in range(size):
            if annotated:
                signature = '(callback: Unary, value: Int) -> Int'
            else:
                signature = '(callback, value)'
            body = 'callback(value)' if index == 0 else f'callback(compose_{index - 1}(callback, value))'
            lines.append(f'pure compose_{index}{signature} {{ {body} }}')
        result_type = ': Int' if annotated else ''
        lines.extend(('', f'let answer{result_type} = compose_{size - 1}(increment, 0)',
                      'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('higher-order-composition', 'chain', size, source(True), source(False))


def _generic_stream(size):
    def source(annotated):
        signature = '(value: Int, count: Int) -> Stream[Int]' if annotated else '(value, count)'
        list_type = ': List[Int]' if annotated else ''
        lines = [f'stream repeated{signature} {{',
                 '  repeat count times {',
                 '    yield value',
                 '  }',
                 '}',
                 '',
                 f'let sevens{list_type} = repeated(7, {size}) |> collect()',
                 f'let nines{list_type} = repeated(9, {size}) |> collect()',
                 'print $sevens.len()',
                 'print $nines.len()',
                 '']
        return {'main.xsh': '\n'.join(lines)}

    return _workload('generic-stream', 'items', size, source(True), source(False))


def _recursive_effect_component(size):
    def source(annotated):
        lines = []
        for index in range(size):
            next_index = (index + 1) % size
            argument_type = ': Int' if annotated else ''
            effect_clause = ' [time]' if annotated else ''
            return_type = ' -> Int' if annotated else ''
            recursive_call = f'effect_step_{next_index}(value - 1)'
            if index == size - 1:
                body = f'if value <= 0 {{ 42 }} else {{ let _ = time.now(); {recursive_call} }}'
            else:
                body = f'if value <= 0 {{ 42 }} else {{ {recursive_call} }}'
            lines.append(f'proc effect_step_{index}(value{argument_type}){effect_clause}{return_type} {{ {body} }}')
        answer_type = ': Int' if annotated else ''
        lines.extend(('', f'let answer{answer_type} = effect_step_0({size})', 'print $answer', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('recursive-effect-component', 'component', size, source(True), source(False))


def _module_diamond(size):
    def sources(annotated):
        result = {'identity.xsh': ('##! Shared identity module.\n'
                                   '## Returns its input unchanged.\n'
                                   'export pure identity(value) { value }\n')}
        imports = []
        terms = []
        for index in range(size):
            for branch in ('left', 'right'):
                module = f'diamond_{index}_{branch}'
                alias = f'{branch}_{index}'
                result[f'{module}.xsh'] = (
                    '##! Branch of the shared identity workload.\n'
                    f'use identity as shared\n'
                    '## Exposes the value returned by the identity function.\n'
                    f'export let value{": Int" if annotated else ""} = shared.identity(1)\n'
                )
                imports.append(f'use {module} as {alias}')
                terms.append(f'{alias}.value')
        answer_type = ': Int' if annotated else ''
        totals = [f'let total_0 = {terms[0]} + {terms[1]}']
        totals.extend(
            f'let total_{index} = total_{index - 1} + {terms[2 * index]} + {terms[2 * index + 1]}'
            for index in range(1, size)
        )
        result['main.xsh'] = '\n'.join((*imports, '', *totals,
                                         f'let answer{answer_type} = total_{size - 1}',
                                         'print $answer', ''))
        return result

    return _workload('module-diamond', 'diamonds', size, sources(True), sources(False))


def _finite_overload_chain(size):
    def source(annotated):
        lines = ['pure identity(value: Int) -> Int { value }' if annotated else 'pure identity(value) { value }']
        list_type = ': List[Int]' if annotated else ''
        lines.append(f'let items_0{list_type} = [1]')
        lines.extend(f'let items_{index}{list_type} = items_{index - 1} |> map(identity)'
                     for index in range(1, size + 1))
        lines.extend(('', f'print $items_{size}.len()', ''))
        return {'main.xsh': '\n'.join(lines)}

    return _workload('finite-overload-chain', 'chain', size, source(True), source(False))


def _named_generic_constructors(size, shape):
    def source(annotated):
        if shape == 'independent':
            types = ['Int' if index % 2 == 0 else 'Str' for index in range(size)]
            values = ['7' if ty == 'Int' else '"kept"' for ty in types]
            parameters = ', '.join(f'value_{index}' + (f': {ty}' if annotated else '')
                                   for index, ty in enumerate(types))
            lines = ['type Box[T] = {value: T}', f'pure build({parameters}) {{']
            lines.extend(f'  let box_{index} = Box(value: value_{index})' for index in range(size))
            lines.extend(('  {' + ', '.join(f'box_{index}' for index in range(size)) + '}', '}'))
            lines.append('let result = build(' + ', '.join(values) + ')')
            lines.extend(f'assert result.box_{index}.value == {value}' for index, value in enumerate(values))
            lines.append('print $result.box_0.value')
        elif shape == 'phantom-context':
            lines = ['type Phantom[T] = {value: Int}', f'pure build(payload{": Int" if annotated else ""}) {{']
            for index in range(size):
                lines.extend((f'  let item_{index}: Phantom[Int] = Phantom(value: {index})',
                              f'  assert item_{index}.value == {index}'))
            lines.extend(('  Ok(payload)', '}', 'let result = build(7)?', 'assert result == 7', 'print $result'))
        else:
            if shape == 'shared-default':
                lines = ['type Box[T] = {value: T, extra: Int = 9, items: List[T] = []}']
                constructor = 'Box(value:)'
            else:
                lines = ['nominal type Brand = {value: Int}', 'type Tagged[T] = Brand',
                         'type Branded[T] = {payload: T, tag: Tagged[T]}',
                         'pure closed_brand() -> Brand { Brand(value: 7) }']
                constructor = 'Branded(payload: value, tag: closed_brand())'
            lines.append(f'pure build(value{": Int" if annotated else ""}) {{')
            lines.extend(f'  let box_{index} = {constructor}' for index in range(size))
            lines.extend(('  {' + ', '.join(f'box_{index}' for index in range(size)) + '}', '}',
                          'let result = build(7)'))
            for index in range(size):
                if shape == 'shared-default':
                    lines.extend((f'assert result.box_{index}.value == 7',
                                  f'assert result.box_{index}.extra == 9',
                                  f'assert result.box_{index}.items.len() == 0'))
                else:
                    lines.extend((f'assert result.box_{index}.payload == 7',
                                  f'assert result.box_{index}.tag.value == 7'))
            lines.append('print $result.box_0.' + ('extra' if shape == 'shared-default' else 'tag.value'))
        return {'main.xsh': '\n'.join((*lines, ''))}

    return _workload('named-generic-constructors', shape, size, source(True), source(False))


def fixed_inference_workloads():
    workloads = []
    for size in SCALING_SIZES:
        workloads.extend((
            _alias_chain(size),
            _dependency_chain(size),
            _wide_record(size),
            _deep_record(size),
            _higher_order_composition(size),
            _generic_stream(size),
            _recursive_effect_component(size),
            _module_diamond(size),
            _finite_overload_chain(size),
        ))
        workloads.extend(_named_generic_constructors(size, shape) for shape in
                         ('independent', 'shared-default', 'ground-carrier', 'phantom-context'))
    return tuple(workloads)
