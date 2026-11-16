#!/usr/bin/env python3
"""Check a packaged PDF helper supports both path and agent file-descriptor input."""
import os
from pathlib import Path
import subprocess
import sys

helper = Path(sys.argv[1]).resolve()
pdf = Path(__file__).resolve().parents[2] / 'native/atlas-native-storage/testdata/two-pages.pdf'
by_path = subprocess.run([helper, pdf], capture_output=True, check=True).stdout
assert by_path.strip(), 'path extraction returned no text'
with pdf.open('rb') as source:
    fd = source.fileno()
    def setup():
        if fd != 3:
            os.dup2(fd, 3)
    by_fd = subprocess.run([helper, '--input-fd'], pass_fds=(fd,), preexec_fn=setup,
                           capture_output=True, check=True).stdout
assert by_fd == by_path, 'descriptor and path extraction differ'
print('PDF extraction works through both a path and an open descriptor')
