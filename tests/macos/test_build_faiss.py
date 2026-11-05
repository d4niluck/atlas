#!/usr/bin/env python3
"""Exercise the macOS FAISS builder with real, checked-out upstream sources.

Run: python3 tests/macos/test_build_faiss.py /path/to/faiss /path/to/llvm-project
CMake, Xcode command-line tools and Python must be available in PATH.
"""
import pathlib
import subprocess
import sys
import tempfile
import unittest

FAISS_SOURCE = LLVM_SOURCE = None
if __name__ == '__main__':
    if len(sys.argv) != 3:
        raise SystemExit(__doc__)
    FAISS_SOURCE = pathlib.Path(sys.argv[1]).resolve()
    LLVM_SOURCE = pathlib.Path(sys.argv[2]).resolve()
    sys.argv = [sys.argv[0]]
SCRIPT = pathlib.Path(__file__).resolve().parents[2] / 'scripts/macos/build-faiss.sh'


def call(source, llvm, output):
    return subprocess.run(
        ['bash', str(SCRIPT), str(source), str(llvm), str(output)],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=1200,
    )


def min_macos_version(path):
    details = subprocess.check_output(['otool', '-l', str(path)], text=True)
    lines = details.splitlines()
    for i, line in enumerate(lines):
        if line.strip() == 'cmd LC_BUILD_VERSION':
            for item in lines[i+1:i+8]:
                if item.strip().startswith('minos '):
                    return tuple(map(int, item.split()[1].split('.')))
        if line.strip() == 'cmd LC_VERSION_MIN_MACOSX':
            for item in lines[i+1:i+5]:
                if item.strip().startswith('version '):
                    return tuple(map(int, item.split()[1].split('.')))
    raise AssertionError(f'No macOS deployment target in {path}')


@unittest.skipUnless(FAISS_SOURCE and LLVM_SOURCE, 'Run this test directly with source paths')
class MacosFaissBuild(unittest.TestCase):
    def setUp(self):
        self.assertTrue(SCRIPT.is_file(), f'Missing builder: {SCRIPT}')

    def test_rejects_each_unexpected_revision(self):
        for faiss, llvm in ((LLVM_SOURCE, LLVM_SOURCE), (FAISS_SOURCE, FAISS_SOURCE)):
            with self.subTest(faiss=faiss, llvm=llvm), tempfile.TemporaryDirectory() as tmp:
                output = pathlib.Path(tmp) / 'output'
                result = call(faiss, llvm, output)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertFalse((output / 'libfaiss.dylib').exists(), result.stdout)

    def test_existing_output_is_untouched(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = pathlib.Path(tmp) / 'output'
            output.mkdir()
            sentinel = output / 'existing'
            sentinel.write_text('keep')
            result = call(FAISS_SOURCE, LLVM_SOURCE, output)
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertEqual(sentinel.read_text(), 'keep')

    def test_real_build_is_relocatable_and_targets_macos_14(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = pathlib.Path(tmp) / 'output'
            result = call(FAISS_SOURCE, LLVM_SOURCE, output)
            self.assertEqual(result.returncode, 0, result.stdout)
            faiss = output / 'libfaiss.dylib'
            omp = output / '.dylibs/libomp.dylib'
            self.assertTrue(faiss.is_file())
            self.assertTrue(omp.is_file())
            dependencies = subprocess.check_output(['otool', '-L', str(faiss)], text=True)
            self.assertIn('@loader_path/.dylibs/libomp.dylib', dependencies)
            self.assertNotIn('/opt/homebrew/', dependencies)
            for library in (faiss, omp):
                self.assertLessEqual(min_macos_version(library), (14, 0, 0))
                architectures = subprocess.check_output(['lipo', '-archs', str(library)], text=True)
                self.assertIn('arm64', architectures.split())


if __name__ == '__main__':
    unittest.main(verbosity=2)
