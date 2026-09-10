#!/usr/bin/env python3
"""Link the diagnostic adapter against existing external native objects, read-only."""
import argparse
import json
from pathlib import Path
from run_release_canaries import execute, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native-build', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build, out = args.native_build.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    source = Path(__file__).resolve().parents[1] / 'examples/precision/native_distance_probe.cpp'
    # Make expands the build's actual flags/object list. There are deliberately
    # no prerequisites, so this target cannot rebuild or write native objects.
    makefile = out / 'Probe.mk'
    makefile.write_text(f'''include {build / 'Makefile'}
.PHONY: fpt-distance-probe
fpt-distance-probe:
\t$(CXX) $(CXXFLAGS) $(INCPATH) -fno-fast-math -c "{source}" -o "{out / 'probe.o'}"
\t$(LINK) $(LFLAGS) -o "{out / 'native-distance-probe'}" "{out / 'probe.o'}" $(filter-out main.o,$(OBJECTS)) $(LIBS)
''')
    command = ['make', '-C', str(build), '-f', str(makefile), 'fpt-distance-probe']
    execute(command, out / 'build', 180)
    binary = out / 'native-distance-probe'
    if not binary.is_file():
        raise RuntimeError('native link failed; inspect build logs')
    (out / 'identity.json').write_text(json.dumps({
        'source_sha256': sha256(source), 'makefile_sha256': sha256(build / 'Makefile'),
        'binary_sha256': sha256(binary),
        'objects': {p.name: sha256(p) for p in build.glob('*.o') if p.name != 'main.o'},
        'scope': 'Unmodified prebuilt native objects; native build arithmetic flags retained. Adapter compiled without fast-math. External GPL-linked diagnostic, not a release artifact.',
    }, indent=2) + '\n')
    print(binary)


if __name__ == '__main__':
    main()
