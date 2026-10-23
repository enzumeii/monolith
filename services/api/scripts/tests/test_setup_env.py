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
elif args == ['get', 'item', os.environ.get('EXPECTED_ITEM', '87323839-368f-4288-a4a5-9b0320699892'), '--nointeraction']:
    expected = os.environ.get('EXPECTED_SESSION')
    if expected: assert os.environ.get('BW_SESSION') == expected
    if os.environ.get('FAIL') == 'get':
        print('synthetic-secret-diagnostic', file=sys.stderr); sys.exit(1)
    if os.environ.get('CONCURRENT'):
        Path(os.environ.get('ENV_FILE', '.env')).write_text('OTHER=concurrent\n')
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

    def test_explicit_local_preserves_existing_setup(self):
        self.assertIn('RUN_ENV="local"', self.assert_success(self.run_setup('--local')))

    def test_qa_uses_own_item_file_and_preserves_values(self):
        local = b'STRIPE_SECRET_KEY=sk_live_synthetic\nJWT_SECRET=local-secret\n'
        (self.cwd / '.env').write_bytes(local)
        self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': 'sk_test_synthetic'},
                     {'name': 'JWT_SECRET', 'value': 'qa-vault-secret'}])
        result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7')
        self.assertEqual(result.returncode, 0, result.stderr)
        path = self.cwd / '.env.qa'
        text = path.read_text()
        self.assertIn('RUN_ENV="qa"', text)
        self.assertIn('STRIPE_SECRET_KEY="sk_test_synthetic"', text)
        self.assertIn('JWT_SECRET="qa-vault-secret"', text)
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual((self.cwd / '.env').read_bytes(), local)
        self.assertEqual(self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7').returncode, 0)
        self.assertEqual(path.read_text(), text)
        self.assertNotIn('sk_test_synthetic', result.stdout + result.stderr)

    def test_qa_without_cli_requires_own_test_key(self):
        (self.bin / 'bw').unlink()
        (self.cwd / '.env').write_text('STRIPE_SECRET_KEY=sk_test_local\n')
        result = self.run_setup('--qa')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.cwd / '.env.qa').exists())
        (self.cwd / '.env.qa').write_text('STRIPE_SECRET_KEY=rk_test_existing\n')
        result = self.run_setup('--qa')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('STRIPE_SECRET_KEY=rk_test_existing', (self.cwd / '.env.qa').read_text())

    def test_qa_rejects_unsafe_existing_file_without_changing_either_file(self):
        (self.bin / 'bw').unlink()
        local = b'OTHER=local\n'
        (self.cwd / '.env').write_bytes(local)
        for content in ('RUN_ENV=local\nSTRIPE_SECRET_KEY=sk_test_safe\n',
                        'STRIPE_SECRET_KEY=sk_live_synthetic\n',
                        'STRIPE_SECRET_KEY=pk_test_synthetic\n',
                        'STRIPE_SECRET_KEY=sk_test_\n',
                        'STRIPE_SECRET_KEY=${STRIPE_SECRET_KEY}\n'):
            with self.subTest(content=content):
                path = self.cwd / '.env.qa'
                path.write_text(content)
                result = self.run_setup('--qa')
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(path.read_text(), content)
                self.assertEqual((self.cwd / '.env').read_bytes(), local)
                self.assertNotIn('synthetic', result.stdout + result.stderr)

    def test_qa_symlink_concurrent_update_and_vault_failure_preserve_files(self):
        local = self.cwd / '.env'
        local.write_text('OTHER=local\n')
        qa = self.cwd / '.env.qa'
        qa.symlink_to(local)
        result = self.run_setup('--qa')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(qa.is_symlink())
        self.assertEqual(local.read_text(), 'OTHER=local\n')
        qa.unlink()
        original = 'STRIPE_SECRET_KEY=sk_test_existing\n'
        qa.write_text(original)
        self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': 'sk_test_synthetic'}])
        for stage in ('status', 'get'):
            result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7', FAIL=stage)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(qa.read_text(), original)
            self.assertNotIn('synthetic-secret-diagnostic', result.stdout + result.stderr)
        result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7', CONCURRENT='1', ENV_FILE='.env.qa')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(qa.read_text(), 'OTHER=concurrent\n')
        self.assertEqual(local.read_text(), 'OTHER=local\n')
        self.assertFalse(list(self.cwd.glob('.env.*.*')))

    def test_qa_rejects_missing_or_unsafe_vault_test_key(self):
        for key in ('', 'sk_live_synthetic', 'rk_live_synthetic', 'pk_test_synthetic', 'sk_test_'):
            self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': key}, {'name': 'JWT_SECRET', 'value': 'synthetic'}])
            result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7')
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.cwd / '.env.qa').exists())
            self.assertNotIn('synthetic', result.stdout + result.stderr)

    def test_imports_same_name_fields_and_keeps_private_file(self):
        text = self.assert_success(self.run_setup(BW_SESSION='inherited', EXPECTED_SESSION='inherited'))
        self.assertIn('SHOPIFY_ACCESS_TOKEN="synthetic-vault-token"', text)
        self.assertIn('S3_ACCESS_KEY_ID="synthetic-local-access"', text)
        self.assertIn('S3_SECRET_ACCESS_KEY="synthetic-local-secret"', text)
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
        self.assertIn('S3_ACCESS_KEY_ID="synthetic-local-access"', text)
        self.assertIn('S3_SECRET_ACCESS_KEY="synthetic-local-secret"', text)
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

    def test_invalid_mode_arguments_fail_without_writes(self):
        for args in (('local',), ('bitwarden',), ('bad',), ('--qa', '--local'), ('--qa', 'excess')):
            result = self.run_setup(*args)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((self.cwd / '.env').exists())

    def test_excluded_vault_fields_leave_local_defaults_and_existing_values(self):
        excluded = ['RUN_ENV', 'DATABASE_URL', 'S3_ACCESS_KEY_ID', 'S3_SECRET_ACCESS_KEY', *(f'SHIPPING_SENDER_{field}' for field in (
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
        self.assertIn('S3_ACCESS_KEY_ID="synthetic-local-access"', text)
        self.assertIn('S3_SECRET_ACCESS_KEY="synthetic-local-secret"', text)
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
        for args, present in (((), True), (('--local',), True), (('--qa',), True), ((), False)):
            with self.subTest(args=args, cli_present=present):
                if not present:
                    (self.bin / 'bw').unlink()
                qa = args == ('--qa',)
                if qa:
                    self.fields([{'name': 'SHOPIFY_ACCESS_TOKEN', 'value': 'synthetic-vault-token'},
                                 {'name': 'STRIPE_SECRET_KEY', 'value': 'sk_test_synthetic'}])
                selected = api / ('.env.qa' if qa else '.env')
                child_env = self.env | ({'EXPECTED_ITEM': '0a291e2c-4ae4-436d-8730-ddf301a544d7'} if qa else {})
                result = subprocess.run([just, '--justfile', str(api / 'justfile'), 'setup', *args],
                                        cwd=api, env=child_env, stdin=subprocess.DEVNULL, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                text = selected.read_text()
                self.assertIn('SHOPIFY_ACCESS_TOKEN="' + ('synthetic-vault-token' if present else '') + '"', text)
                selected.unlink()
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


class RunEnvTests(unittest.TestCase):
    def setUp(self):
        self.just = shutil.which('just')
        if not self.just:
            self.skipTest('just is required for recipe integration')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.api = self.root / 'services' / 'api'
        self.api.mkdir(parents=True)
        shutil.copy(SCRIPT.parents[1] / 'justfile', self.api / 'justfile')
        (self.api / 'application.qa.toml').write_text('[client.stripe]\nbase_url="https://api.stripe.com/v1/"\n')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        (self.bin / 'bash').symlink_to('/bin/bash')
        (self.bin / 'just').symlink_to(self.just)
        self.env = dict(PATH=str(self.bin) + os.pathsep + os.path.dirname(sys.executable), HOME=str(self.root),
                        PYTHONDONTWRITEBYTECODE='1', CAPTURE=str(self.root / 'events.jsonl'))
        self.command('docker', "from pathlib import Path\nif 'ps' in sys.argv:\n    marker = Path('.docker-started')\n    print('running' if marker.exists() or os.environ.get('S3_EXISTS') else '')\n    marker.touch()\n")
        self.command('sleep', '')
        self.command('cargo', "sys.exit(int(os.environ.get('CARGO_FAIL', '0')))\n")
        target = self.root / 'target' / 'debug'
        target.mkdir(parents=True)
        self.command('api', '', target)

    def command(self, name, behavior, directory=None):
        path = (directory or self.bin) / name
        path.write_text('#!/usr/bin/env python3\n' +
                        'import json, os, sys\n' +
                        "with open(os.environ['CAPTURE'], 'a') as output:\n" +
                        "    output.write(json.dumps({'command': os.path.basename(sys.argv[0]), 'args': sys.argv[1:], 'env': dict(os.environ)}) + '\\n')\n" + behavior)
        path.chmod(0o755)

    def run_recipe(self, *args, **env):
        return subprocess.run([self.just, '--justfile', str(self.api / 'justfile'), *args],
                              cwd=self.api, env=self.env | env, capture_output=True, text=True, timeout=15)

    def events(self):
        path = self.root / 'events.jsonl'
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

    def test_qa_sanitizes_parent_and_outer_dotenv_for_migration_and_api(self):
        names = ('DATABASE_URL', 'JWT_SECRET', 'CRON_SECRET', 'SHOPIFY_ACCESS_TOKEN', 'RESEND_API_KEY',
                 'PAYPAL_CLIENT_ID', 'PAYPAL_CLIENT_SECRET', 'STRIPE_SECRET_KEY', 'S3_ACCESS_KEY_ID',
                 'S3_SECRET_ACCESS_KEY', 'SHIPPING_SENDER_NAME', 'SHIPPING_SENDER_PHONE',
                 'SHIPPING_SENDER_EMAIL', 'SHIPPING_SENDER_ADDRESS', 'SHIPPING_SENDER_SUB_DISTRICT',
                 'SHIPPING_SENDER_DISTRICT', 'SHIPPING_SENDER_PROVINCE', 'SHIPPING_SENDER_POSTCODE',
                 'SHIPPING_SENDER_COUNTRY_CODE')
        inherited = {name: 'inherited-local' for name in names}
        inherited.update(STRIPE_SECRET_KEY='sk_live_synthetic', RUN_ENV='prod', RUST_LOG='debug',
                         BW_SESSION='synthetic-session', E2E_COMPOSE_PROJECT='synthetic-project')
        (self.api / '.env').write_text(''.join(f'{name}=outer-local\n' for name in names))
        (self.api / '.env.qa').write_text('RUN_ENV=prod\nDATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=sk_test_synthetic\nJWT_SECRET=qa-jwt\n')
        result = self.run_recipe('run', '--qa', **inherited)
        self.assertEqual(result.returncode, 0, result.stderr)
        consumers = [event for event in self.events() if event['command'] in ('cargo', 'api')]
        self.assertEqual([event['args'] for event in consumers[:2]],
                         [['sqlx', '--no-dotenv', 'migrate', 'run'], ['build', '--locked', '-p', 'api', '--bin', 'api']])
        self.assertTrue(any(event['command'] == 'api' for event in consumers))
        for event in consumers:
            child = event['env']
            self.assertEqual(child['RUN_ENV'], 'qa')
            self.assertEqual(child['DATABASE_URL'], 'qa-db')
            self.assertEqual(child['STRIPE_SECRET_KEY'], 'sk_test_synthetic')
            self.assertEqual(child['JWT_SECRET'], 'qa-jwt')
            for name in names:
                if name not in ('DATABASE_URL', 'STRIPE_SECRET_KEY', 'JWT_SECRET'):
                    self.assertNotIn(name, child)
            for name in ('RUST_LOG', 'BW_SESSION', 'E2E_COMPOSE_PROJECT'):
                self.assertEqual(child[name], inherited[name])
        self.assertNotIn('sk_test_synthetic', result.stdout + result.stderr)
        self.assertEqual(self.events()[-1]['args'], ['compose', '-f', 'docker-compose.yml', 'stop', 'local-s3'])

    def test_local_alias_and_flag_preserve_explicit_environment_precedence(self):
        (self.api / '.env').write_text('RUN_ENV=prod\nDATABASE_URL=file-db\nSTRIPE_SECRET_KEY=sk_live_file\n')
        for args in (('run',), ('run', '--local')):
            with self.subTest(args=args):
                result = self.run_recipe(*args, DATABASE_URL='explicit-db', STRIPE_SECRET_KEY='sk_live_explicit', RUN_ENV='qa', S3_EXISTS='1')
                self.assertEqual(result.returncode, 0, result.stderr)
        consumers = [event for event in self.events() if event['command'] in ('cargo', 'api')]
        for event in consumers:
            self.assertEqual(event['env']['RUN_ENV'], 'local')
            self.assertEqual(event['env']['DATABASE_URL'], 'explicit-db')
            self.assertEqual(event['env']['STRIPE_SECRET_KEY'], 'sk_live_explicit')
        self.assertFalse(any(event['args'][-2:] == ['stop', 'local-s3'] for event in self.events()))

    def test_missing_qa_files_or_database_and_unsafe_keys_fail_before_docker(self):
        (self.api / '.env').write_text('DATABASE_URL=local-db\nSTRIPE_SECRET_KEY=sk_test_local\n')
        path = self.api / '.env.qa'
        for text in (None, 'DATABASE_URL=qa-db\n', 'STRIPE_SECRET_KEY=sk_test_synthetic\n',
                     'DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=sk_live_synthetic\n',
                     'DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=pk_test_synthetic\n',
                     'DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=sk_test_\n',
                     'DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY="sk_test_bad space"\n'):
            with self.subTest(text=text):
                if text is not None:
                    path.write_text(text)
                result = self.run_recipe('run', '--qa')
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.events(), "A dependency command ran before validation")
        path.write_text('DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=rk_test_synthetic\n')
        (self.api / 'application.qa.toml').unlink()
        result = self.run_recipe('run', '--qa')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.events(), "A dependency command ran before validation")

    def test_invalid_recipe_flags_fail_before_setup_or_docker(self):
        for recipe in ('setup', 'run'):
            for args in (('local',), ('--bad',), ('--qa', '--local'), ('--local', 'excess')):
                with self.subTest(recipe=recipe, args=args):
                    result = self.run_recipe(recipe, *args)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(self.events(), "A dependency command ran before validation")
                    self.assertFalse((self.api / '.env').exists())
                    self.assertFalse((self.api / '.env.qa').exists())

    def test_failed_migration_runs_owned_s3_cleanup(self):
        (self.api / '.env.qa').write_text('DATABASE_URL=qa-db\nSTRIPE_SECRET_KEY=rk_test_synthetic\n')
        result = self.run_recipe('run', '--qa', CARGO_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.events(), result.stderr)
        self.assertEqual(self.events()[-1]['args'], ['compose', '-f', 'docker-compose.yml', 'stop', 'local-s3'])
        self.assertFalse(any(event['command'] == 'api' for event in self.events()))


if __name__ == '__main__':
    unittest.main()
