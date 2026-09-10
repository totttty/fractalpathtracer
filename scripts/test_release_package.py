from pathlib import Path
import tomllib
import unittest


class PackageScopeTests(unittest.TestCase):
    def test_root_file_patterns_cannot_collect_report_readmes(self):
        root = Path(__file__).resolve().parents[1]
        package = tomllib.loads((root/'Cargo.toml').read_text())['package']
        patterns = package['include']
        for name in ('Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain.toml',
                     'README.md', 'LICENSE', 'NOTICE'):
            self.assertIn('/'+name, patterns)
            self.assertNotIn(name, patterns)
        self.assertFalse(any(p.lstrip('/').startswith(('reports/', 'target/'))
                             for p in patterns))
