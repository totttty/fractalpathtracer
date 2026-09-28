"""Conservative, content-bound native reference cache. Never cache failures."""
import hashlib
import json
import os
import shutil
import tempfile
from pathlib import Path

from run_release_canaries import image_result, sha256


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


# Variables that can change what the native renderer reads or computes. Hashing the whole
# environment made every app or session restart (new socket paths, tokens) a cache miss.
NATIVE_ENVIRONMENT_KEYS = ('HOME', 'LANG', 'TZ')
NATIVE_ENVIRONMENT_PREFIXES = ('LC_', 'QT_', 'OMP_', 'KMP_', 'DYLD_', 'MANDELBULBER', 'OCL_', 'OPENCL_')


def native_environment(environ=None):
    environ = os.environ if environ is None else environ
    # Only the digest is stored, never the values (which could be sensitive).
    return {k: v for k, v in environ.items()
            if k in NATIVE_ENVIRONMENT_KEYS or k.startswith(NATIVE_ENVIRONMENT_PREFIXES)}


def reference_contract(command, scene, size, lightmap, shared_root):
    # The full scene hash covers authored camera, projection and all settings.
    # Fail closed for external assets whose actual native resolution is unknown.
    for line in scene.read_text().splitlines():
        parts = line.strip().rstrip(';').split(None, 1)
        if len(parts) != 2:
            continue
        key, value = parts
        if 'file_' in key and key not in ('file_lightmap', 'file_destination') and value:
            return None, 'authored external asset lacks a pinned native resolution: '+key
    normalized = list(command)
    normalized[normalized.index('-o')+1] = '<output>'
    textures = shared_root/'deploy/share/mandelbulber2/textures'
    if not textures.is_dir():
        return None, 'native shared textures directory unavailable'
    roots = [textures]
    # macOS uses bundle resources, not necessarily the supplied source tree.
    bundle = Path(command[0]).parent.parent/'Resources'
    if bundle.is_dir():
        roots.extend(bundle/name for name in ('textures','data','materials'))
    assets = {str(p.resolve()): sha256(p) for root in roots
              for p in sorted(root.rglob('*')) if p.is_file()}
    assets[lightmap['path']] = sha256(Path(lightmap['path']))
    settings = {str(p.resolve()): sha256(p)
                for root in (Path.home()/'mandelbulber', Path.home()/'.mandelbulber')
                for p in sorted(root.glob('*.ini')) if p.is_file()}
    return dict(version=2, scene_sha256=sha256(scene), dimensions=list(size),
                command=normalized, binary_sha256=sha256(Path(command[0])),
                assets=assets, native_settings=settings,
                resource_roots=[str(root.resolve()) for root in roots],
                environment_sha256=digest(native_environment())), None


def verify_native_log(contract, folder):
    """Reject provenance that names a native settings/resource root we did not pin."""
    text=(folder/'stdout.log').read_text()
    if 'OpenCl - rendering' in text:
        raise ValueError('OpenCL used despite CPU override')
    for line in text.splitlines():
        if line.startswith('Settings file: '):
            path=str(Path(line.split(': ',1)[1]).resolve())
            if path not in contract['native_settings']:
                raise ValueError('native settings path was not fingerprinted: '+path)
        if line.startswith('sharePath directory: '):
            path=str((Path(line.split(': ',1)[1])/'textures').resolve())
            if path not in contract['resource_roots']:
                raise ValueError('native resource root was not fingerprinted: '+path)


def load_reference(cache, contract, folder):
    """A corrupt or mismatched entry is a miss, never a successful reference."""
    entry = cache/digest(contract)
    try:
        manifest = json.loads((entry/'entry.json').read_text())
        if manifest['contract'] != contract or manifest['result']['status'] != 'ok':
            return None
        for name, expected in manifest['files'].items():
            if Path(name).name != name or sha256(entry/name) != expected:
                return None
        if not {'scene.png', 'stdout.log', 'stderr.log', 'command.json'} <= manifest['files'].keys():
            return None
        actual = image_result(entry, tuple(contract['dimensions']))
        if actual['sha256'] != manifest['result']['capture']['sha256']:
            return None
        recorded = json.loads((entry/'command.json').read_text())
        recorded[recorded.index('-o')+1] = '<output>'
        if recorded != contract['command']:
            return None
        verify_native_log(contract,entry)
    except (OSError, ValueError, KeyError, TypeError):
        return None
    folder.mkdir(parents=True, exist_ok=False)
    for name in manifest['files']:
        shutil.copy2(entry/name, folder/name)
        if sha256(folder/name) != manifest['files'][name]:
            raise ValueError('cached artifact changed during copy: '+name)
    result = manifest['result']
    result['capture'] = dict(image_result(folder, tuple(contract['dimensions'])), wall_seconds=0.0)
    result['reference_cache'] = dict(status='hit', key=digest(contract),
        original_wall_seconds=manifest['result'].get('original_wall_seconds', 0),
        manifest_sha256=sha256(entry/'entry.json'))
    return result


def store_reference(cache, contract, folder, result):
    if result['status'] != 'ok':
        return
    verify_native_log(contract,folder)
    cache.mkdir(parents=True, exist_ok=True)
    entry = cache/digest(contract)
    if entry.exists():
        return  # Preserve prior evidence, including a corrupt entry for diagnosis.
    with tempfile.TemporaryDirectory(prefix='.native-', dir=cache) as temp:
        stage = Path(temp)
        files = {}
        for name in ('scene.png', 'stdout.log', 'stderr.log', 'command.json'):
            shutil.copy2(folder/name, stage/name)
            files[name] = sha256(stage/name)
        stored = dict(result, original_wall_seconds=result['capture']['wall_seconds'])
        (stage/'entry.json').write_text(json.dumps(dict(contract=contract, result=stored, files=files), indent=2))
        try:
            stage.rename(entry)
        except FileExistsError:
            pass
