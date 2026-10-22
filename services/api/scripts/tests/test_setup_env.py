import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / 'setup-env.py'
FAKE_BW = r'''#!/usr/bin/env python3
import json, os, sys, termios
from pathlib import Path
args = sys.argv[1:]
if args == ['status', '--nointeraction']:
    if os.environ.get('FAIL') == 'status':
        print('synthetic-secret-diagnostic', file=sys.stderr); sys.exit(1)
    print(os.environ.get('STATUS_JSON', json.dumps({'status': os.environ.get('STATUS', 'unlocked')})))
elif args == ['unlock', '--raw']:
    assert 'BW_SESSION' not in os.environ
    assert sys.stdin.isatty() and sys.stderr.isatty()
    settings = termios.tcgetattr(0); hidden = termios.tcgetattr(0)
    hidden[3] &= ~termios.ECHO
    termios.tcsetattr(0, termios.TCSANOW, hidden)
    print('Master password:', file=sys.stderr, flush=True)
    password = input()
    termios.tcsetattr(0, termios.TCSANOW, settings)
    if password != 'synthetic-password' or os.environ.get('FAIL') == 'unlock': sys.exit(1)
    print('synthetic-session')
elif args == ['get', 'item', '87323839-368f-4288-a4a5-9b0320699892', '--nointeraction']:
    expected = os.environ.get('EXPECTED_SESSION')
    if expected: assert os.environ.get('BW_SESSION') == expected
    if os.environ.get('FAIL') == 'get':
        print('synthetic-secret-diagnostic', file=sys.stderr); sys.exit(1)
    if os.environ.get('CONCURRENT'):
        Path('.env').write_text('OTHER=concurrent\n')
    print(Path(os.environ['ITEM']).read_text())
else:
    print('unexpected invocation', file=sys.stderr); sys.exit(1)
'''


class SetupEnvTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.cwd = Path(self.temp.name)
        self.bin = self.cwd / 'bin'
        self.bin.mkdir()
        bw = self.bin / 'bw'
        bw.write_text(FAKE_BW)
        bw.chmod(0o755)
        self.item = self.cwd / 'item.json'
        self.fields([{'name': 'SHOPIFY_ACCESS_TOKEN', 'value': 'synthetic-vault-token', 'type': 1}])
        self.env = {key: value for key, value in os.environ.items() if key not in ('BW_SESSION', 'BW_NOINTERACTION')}
        self.env.update(PATH=str(self.bin) + os.pathsep + os.path.dirname(sys.executable), ITEM=str(self.item), PYTHONDONTWRITEBYTECODE='1')

    def fields(self, fields):
        self.item.write_text(json.dumps({'object': 'item', 'id': 'synthetic-id', 'fields': fields}))

    def run_setup(self, *args, **env):
        return subprocess.run([sys.executable, str(SCRIPT), *args], cwd=self.cwd,
                              env=self.env | env, stdin=subprocess.DEVNULL, capture_output=True, text=True)

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr)
        return (self.cwd / '.env').read_text()

    def test_imports_same_name_fields_and_keeps_private_file(self):
        text = self.assert_success(self.run_setup(BW_SESSION='inherited', EXPECTED_SESSION='inherited'))
        self.assertIn('SHOPIFY_ACCESS_TOKEN="synthetic-vault-token"', text)
        self.assertIn('BLOB_READ_WRITE_TOKEN="vercel_blob_rw_local_synthetic"', text)
        self.assertRegex(text, r'JWT_SECRET="[0-9a-f]{64}"')
        self.assertEqual((self.cwd / '.env').stat().st_mode & 0o777, 0o600)

    def test_preserves_nonempty_assignments_comments_and_multiline_values(self):
        original = '# keep\nexport JWT_SECRET = "kept" # comment\nSHOPIFY_ACCESS_TOKEN=\'\' # empty\nOTHER="multi\nSHOPIFY_ACCESS_TOKEN=inside\nline"\n'
        (self.cwd / '.env').write_text(original)
        text = self.assert_success(self.run_setup())
        self.assertIn('export JWT_SECRET = "kept" # comment', text)
        self.assertIn('OTHER="multi\nSHOPIFY_ACCESS_TOKEN=inside\nline"', text)
        self.assertIn('SHOPIFY_ACCESS_TOKEN="synthetic-vault-token"', text)
        first = (self.cwd / '.env').read_bytes()
        self.assert_success(self.run_setup())
        self.assertEqual((self.cwd / '.env').read_bytes(), first)

    def test_existing_nonempty_duplicate_is_used_for_empty_first_assignment(self):
        (self.cwd / '.env').write_text('JWT_SECRET=\nJWT_SECRET=kept\n')
        text = self.assert_success(self.run_setup())
        self.assertTrue(text.startswith('JWT_SECRET=kept\nJWT_SECRET=kept\n'))

    def test_missing_cli_uses_existing_local_defaults(self):
        (self.bin / 'bw').unlink()
        text = self.assert_success(self.run_setup())
        self.assertIn('RUN_ENV="local"', text)
        self.assertIn('DATABASE_URL="postgres://user:pass@127.0.0.1:5432/core"', text)
        self.assertIn('SHOPIFY_ACCESS_TOKEN=""', text)
        self.assertIn('BLOB_READ_WRITE_TOKEN="vercel_blob_rw_local_synthetic"', text)
        self.assertIn('SHIPPING_SENDER_NAME="Synthetic Sender ', text)
        self.assertRegex(text, r'JWT_SECRET="[0-9a-f]{64}"')
        self.assertRegex(text, r'CRON_SECRET="[0-9a-f]{64}"')

    def test_cron_secret_is_generated_when_missing_or_empty_and_preserved(self):
        for initial in ('', 'CRON_SECRET=\n', 'CRON_SECRET="kept-cron-secret"\n'):
            with self.subTest(initial=initial):
                (self.cwd / '.env').write_text(initial)
                text = self.assert_success(self.run_setup())
                if initial.endswith('"kept-cron-secret"\n'):
                    self.assertIn('CRON_SECRET="kept-cron-secret"', text)
                else:
                    self.assertRegex(text, r'CRON_SECRET="[0-9a-f]{64}"')
                self.assertEqual((self.cwd / '.env').stat().st_mode & 0o777, 0o600)
                self.assertEqual(self.assert_success(self.run_setup()), text)

    def test_cron_secret_imports_vault_value_and_preserves_existing(self):
        self.fields([{'name': 'CRON_SECRET', 'value': 'synthetic-vault-cron', 'type': 1}])
        for initial, expected in (('', 'synthetic-vault-cron'),
                                  ('CRON_SECRET=\n', 'synthetic-vault-cron'),
                                  ('CRON_SECRET="existing-cron"\n', 'existing-cron')):
            with self.subTest(initial=initial):
                (self.cwd / '.env').write_text(initial)
                text = self.assert_success(self.run_setup())
                self.assertIn(f'CRON_SECRET="{expected}"', text)
                self.assertNotIn('synthetic-vault-cron', self.run_setup().stdout)

    def test_mode_arguments_are_not_supported(self):
        for argument in ('local', 'bitwarden', 'bad'):
            result = self.run_setup(argument)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.cwd / '.env').exists())

    def test_excluded_vault_fields_leave_local_defaults_and_existing_values(self):
        excluded = ['RUN_ENV', 'DATABASE_URL', 'BLOB_READ_WRITE_TOKEN', *(f'SHIPPING_SENDER_{field}' for field in (
            'NAME', 'PHONE', 'EMAIL', 'ADDRESS', 'SUB_DISTRICT', 'DISTRICT',
            'PROVINCE', 'POSTCODE', 'COUNTRY_CODE',
        ))]
        fields = [{'name': 'JWT_SECRET', 'value': 'vault-signing'}]
        for name in excluded:
            fields.extend([{'name': name, 'value': 'excluded-vault-value'},
                           {'name': name, 'value': 42, 'type': 3}])
        self.fields(fields)
        text = self.assert_success(self.run_setup())
        self.assertIn('JWT_SECRET="vault-signing"', text)
        self.assertNotIn('excluded-vault-value', text)
        self.assertIn('RUN_ENV="local"', text)
        self.assertIn('DATABASE_URL="postgres://user:pass@127.0.0.1:5432/core"', text)
        self.assertIn('BLOB_READ_WRITE_TOKEN="vercel_blob_rw_local_synthetic"', text)
        self.assertIn('SHIPPING_SENDER_NAME="Synthetic Sender ', text)
        existing = ''.join(f'{name}="existing-value"\n' for name in excluded)
        (self.cwd / '.env').write_text(existing)
        text = self.assert_success(self.run_setup())
        self.assertTrue(text.startswith(existing))

    def test_unauthenticated_and_locked_without_terminal_leave_bytes(self):
        for status, instruction in [('unauthenticated', 'bw login'), ('locked', 'terminal')]:
            with self.subTest(status=status):
                (self.cwd / '.env').write_bytes(b'# untouched\r\n')
                result = self.run_setup(STATUS=status)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(instruction, result.stderr)
                self.assertEqual((self.cwd / '.env').read_bytes(), b'# untouched\r\n')

    def test_cli_failures_do_not_disclose_diagnostics_or_write(self):
        for stage in ('status', 'get'):
            with self.subTest(stage=stage):
                result = self.run_setup(FAIL=stage)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('synthetic-secret-diagnostic', result.stdout + result.stderr)
                self.assertFalse((self.cwd / '.env').exists())

    def test_malformed_status_fails_safely(self):
        for status in ('not json', '{}', '[]', '{"status":"unknown"}'):
            result = self.run_setup(STATUS_JSON=status)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.cwd / '.env').exists())

    def test_malformed_item_and_ambiguous_fields_leave_existing_bytes(self):
        invalid = ['not json', '[]', '{}', '{"fields":[]}', json.dumps({'fields': [{'name': 'UNRELATED', 'value': 'ignored'}]}),
                   json.dumps({'fields': [{'name': 'JWT_SECRET', 'value': 42}]}),
                   json.dumps({'fields': [{'name': 'JWT_SECRET', 'value': 'a', 'type': 3}]}),
                   json.dumps({'fields': [{'name': 'JWT_SECRET', 'value': 'a'}, {'name': 'JWT_SECRET', 'value': 'b'}]})]
        for item in invalid:
            with self.subTest(item=item):
                self.item.write_text(item)
                (self.cwd / '.env').write_bytes(b'# unchanged\n')
                result = self.run_setup()
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual((self.cwd / '.env').read_bytes(), b'# unchanged\n')
                self.assertNotIn(item, result.stderr)

    def test_missing_optional_fields_use_defaults_and_unrelated_fields_are_ignored(self):
        self.fields([{'name': 'JWT_SECRET', 'value': 'vault-signing'}, {'name': 'IGNORE', 'value': 'do-not-copy'}])
        text = self.assert_success(self.run_setup())
        self.assertIn('JWT_SECRET="vault-signing"', text)
        self.assertIn('PAYPAL_CLIENT_SECRET=""', text)
        self.assertNotIn('do-not-copy', text)

    def test_concurrent_update_is_not_overwritten(self):
        (self.cwd / '.env').write_text('OTHER=original\n')
        result = self.run_setup(CONCURRENT='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((self.cwd / '.env').read_text(), 'OTHER=concurrent\n')

    def test_write_failure_does_not_leave_staging_files(self):
        (self.cwd / '.env').mkdir()
        result = self.run_setup()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sorted(p.name for p in self.cwd.iterdir()), ['.env', 'bin', 'item.json'])
        self.assertNotIn('Traceback', result.stderr)

    def test_locked_unlock_uses_native_terminal_and_keeps_session_private(self):
        self.interactive_unlock('synthetic-password', success=True)

    def test_unlock_cancellation_or_wrong_password_does_not_write(self):
        self.interactive_unlock('wrong-password', success=False)

    def test_comment_quotes_do_not_become_multiline_values(self):
        (self.cwd / '.env').write_text("SHOPIFY_ACCESS_TOKEN=# don't parse this quote\n")
        text = self.assert_success(self.run_setup())
        self.assertIn('SHOPIFY_ACCESS_TOKEN="synthetic-vault-token"', text)

    def test_special_characters_are_literal_dotenv_values(self):
        self.fields([{'name': 'JWT_SECRET', 'value': 'space \"quote\" \\path $HOME ${TOKEN} #hash\nnext\ttab'}])
        text = self.assert_success(self.run_setup())
        self.assertIn('JWT_SECRET="space \\\"quote\\\" \\\\path \\$HOME \\${TOKEN} #hash\\nnext\ttab"', text)

    def test_unsupported_control_characters_do_not_write(self):
        for value in ('synthetic\rvalue', 'synthetic\x00value'):
            self.fields([{'name': 'JWT_SECRET', 'value': value}])
            result = self.run_setup()
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.cwd / '.env').exists())

    def test_single_just_setup_works_with_present_or_absent_cli(self):
        just = shutil.which('just')
        if not just:
            self.skipTest('just is required for recipe integration')
        api = self.cwd / 'services' / 'api'
        api.mkdir(parents=True)
        scripts = api / 'scripts'
        scripts.mkdir()
        shutil.copy(SCRIPT, scripts / SCRIPT.name)
        shutil.copy(SCRIPT.parents[1] / 'justfile', api / 'justfile')
        (self.bin / 'bash').symlink_to('/bin/bash')
        cargo = self.bin / 'cargo'
        cargo.write_text("#!/bin/sh\n/bin/mkdir -p .tools/bin\n/bin/echo installed > cargo-ran\n/bin/echo '#!/bin/sh' > .tools/bin/dprint\n/bin/chmod +x .tools/bin/dprint\n")
        cargo.chmod(0o755)
        for present in (True, False):
            with self.subTest(cli_present=present):
                if not present:
                    (self.bin / 'bw').unlink()
                result = subprocess.run([just, '--justfile', str(api / 'justfile'), 'setup'],
                                        cwd=api, env=self.env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                text = (api / '.env').read_text()
                self.assertIn('SHOPIFY_ACCESS_TOKEN="' + ('synthetic-vault-token' if present else '') + '"', text)
                (api / '.env').unlink()
        (api / 'cargo-ran').unlink()
        result = subprocess.run([just, '--justfile', str(api / 'justfile'), 'setup', 'local'],
                                cwd=api, env=self.env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((api / 'cargo-ran').exists())
        self.assertFalse((api / '.env').exists())

    def interactive_unlock(self, password, success):
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        self.addCleanup(os.close, slave)
        process = subprocess.Popen([sys.executable, str(SCRIPT)], cwd=self.cwd,
                                   env=self.env | {'STATUS': 'locked', 'BW_SESSION': 'stale', 'EXPECTED_SESSION': 'synthetic-session'},
                                   stdin=slave, stderr=slave, stdout=subprocess.PIPE)
        self.addCleanup(lambda: process.kill() if process.poll() is None else None)
        self.addCleanup(process.stdout.close)
        transcript = b''
        while b'Master password:' not in transcript:
            ready, _, _ = select.select([master], [], [], 5)
            self.assertTrue(ready, 'native prompt did not appear')
            transcript += os.read(master, 4096)
        os.write(master, (password + '\n').encode())
        output, _ = process.communicate(timeout=5)
        while select.select([master], [], [], 0)[0]:
            transcript += os.read(master, 4096)
        self.assertNotIn(password.encode(), transcript + output)
        self.assertNotIn(b'synthetic-session', transcript + output)
        if success:
            self.assertEqual(process.returncode, 0, transcript)
            self.assertNotIn('synthetic-session', (self.cwd / '.env').read_text())
        else:
            self.assertNotEqual(process.returncode, 0)
            self.assertFalse((self.cwd / '.env').exists())


if __name__ == '__main__':
    unittest.main()
