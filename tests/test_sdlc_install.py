"""Exercise the installed workflow in a throwaway repository, using synthetic decisions."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstalledWorkflowTest(unittest.TestCase):
    def test_later_failed_mark_invalidates_verification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ('_system', '_shared', 'stages'):
                shutil.copytree(ROOT / name, root / name)
            env = {**os.environ, 'GIT_CONFIG_GLOBAL': os.devnull, 'GIT_CONFIG_NOSYSTEM': '1',
                   'GIT_AUTHOR_NAME': 'Synthetic fixture', 'GIT_COMMITTER_NAME': 'Synthetic fixture',
                   'GIT_AUTHOR_EMAIL': 'fixture@example.invalid', 'GIT_COMMITTER_EMAIL': 'fixture@example.invalid'}

            def run(*argv):
                result = subprocess.run(argv, cwd=root, env=env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return result.stdout

            def sdlc(*argv):
                return run(sys.executable, str(root / '_system/scripts/sdlc.py'), *argv)

            (root / '_system/verification.json').write_text(json.dumps({
                'require_tink': False,
                'checks': [{'argv': [sys.executable, '-c', 'pass'], 'timeout_seconds': 10}]}))
            run('git', 'init', '-q')
            run('git', 'add', '.')
            run('git', 'commit', '-qm', 'Synthetic fixture')
            sdlc('new', 'probe')
            (root / 'runs/probe/checklist.json').write_text(json.dumps({'schema': 1, 'items': [
                {'id': 'manual', 'description': 'Fixture check', 'verify': 'Manual inspection'}]}))
            sdlc('decide', 'probe', '3', 'approved', '--reviewer', 'Synthetic fixture',
                 '--source', 'isolated test', '--reason', 'No real approval')
            sdlc('mark', 'probe', 'manual', 'passed', '--evidence', 'Synthetic pass')
            sdlc('verify', 'probe')
            self.assertIn('Verification: current', sdlc('status', 'probe'))
            sdlc('mark', 'probe', 'manual', 'failed', '--evidence', 'Synthetic regression')
            status = sdlc('status', 'probe')
            self.assertIn('manual: failed', status)
            self.assertIn('Verification: failed, stale, or blocked', status)
            self.assertNotIn('Next: independent PR review', status)
