#!/usr/bin/env python3
"""Inventory authored light settings; this is not a render-parity certificate."""
import argparse
from collections import Counter
import json
from pathlib import Path
import re

from run_mandel_support_suite import parameters
from run_release_canaries import sha256


def boolean(value):
    if value.lower() in ('true', '1'):
        return True
    if value.lower() in ('false', '0'):
        return False
    raise ValueError(f'invalid light boolean: {value}')


def inventory(text):
    values = parameters(text)
    match = re.search(r'^# version (\d+)\.(\d+)', text, re.M)
    if not match:
        raise ValueError('missing source version')
    modern = tuple(map(int, match.groups())) >= (2, 25)
    main = {'enabled': boolean(values.get('light1_enabled' if modern else 'main_light_enable', 'true')),
            'type': values.get('light1_type', 'directional') if modern else 'directional',
            'relative_position': boolean(values.get('light1_relative_position' if modern else 'main_light_position_relative', 'true')),
            'use_target_point': boolean(values.get('light1_use_target_point', 'false')) if modern else False}
    lights = []
    pattern = r'light(\d+)_enabled' if modern else r'aux_light_enabled_(\d+)'
    for key, value in values.items():
        found = re.fullmatch(pattern, key)
        if not found or not boolean(value):
            continue
        index = int(found.group(1))
        if modern and index == 1:
            continue
        if modern:
            prefix = f'light{index}_'
            kind = values.get(prefix + 'type', 'point')
            kind = {'0': 'directional', '1': 'point', '2': 'conical', '3': 'projection', '4': 'beam'}.get(kind, kind)
            relative = boolean(values.get(prefix + 'relative_position', 'false'))
        else:
            prefix, kind, relative = f'aux_light_', 'point', False
        lights.append({'native_light_id': index if modern else index + 1,
                       'type': kind, 'relative_position': relative,
                       'continuous_fpt_direct_contribution': 'not implemented',
                       'raw_settings': {k: v for k, v in values.items() if
                           (k.startswith(prefix) if modern else k.startswith(prefix) and k.endswith(f'_{index}'))}})
    # Only explicitly enabled auxiliary sources are counted. Missing legacy
    # flags with other auxiliary fields are surfaced rather than guessed.
    implicit = []
    if not modern:
        for key in values:
            found = re.fullmatch(r'aux_light_(?:position|intensity|colour)_(\d+)', key)
            if found and f'aux_light_enabled_{found.group(1)}' not in values:
                implicit.append(int(found.group(1)))
    return {'source_version': '.'.join(match.groups()), 'main': main,
            'explicit_enabled_auxiliary': sorted(lights, key=lambda light: light['native_light_id']),
            'legacy_implicit_auxiliary_ids_require_review': sorted(set(implicit)),
            'random_lights_enabled': boolean(values.get('random_lights_group', 'false')) if modern else False,
            'legacy_random_light_count': values.get('aux_light_number') if not modern else None,
            'fake_lights_enabled': boolean(values.get('fake_lights_enabled', 'false'))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--audit', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('use a new report path')
    rows = []
    for directory in sorted(args.audit.glob('[0-9][0-9]')):
        command = json.loads((directory / 'mandel/command.json').read_text())
        source = Path(command[-1])
        rows.append({'id': directory.name, 'source': str(source), 'source_sha256': sha256(source),
                     **inventory(source.read_text())})
    counts = Counter(light['type'] for row in rows for light in row['explicit_enabled_auxiliary'])
    result = {'scope': 'Source-setting inventory plus code inspection: continuous renderPath adds sun and ambient but no auxiliary direct sources. FPTVOX appearance metadata is not proof of continuous-renderer support. No native settings migration or visual gate performed by this script.',
              'scene_count': len(rows), 'explicit_auxiliary_type_counts': dict(counts),
              'scenes_with_explicit_auxiliary': [row['id'] for row in rows if row['explicit_enabled_auxiliary']],
              'rows': rows}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ('rows', 'scope')}, indent=2))


if __name__ == '__main__':
    main()
