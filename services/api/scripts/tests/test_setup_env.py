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
        self.env = dict(PATH=str(self.bin) + os.pathsep + os.path.dirname(sys.executable),
                        HOME=str(self.cwd), ITEM=str(self.item), PYTHONDONTWRITEBYTECODE='1')

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

    def test_mode_switch_replaces_only_imports_and_preserves_records(self):
        imported = ('JWT_SECRET', 'CRON_SECRET', 'SHOPIFY_ACCESS_TOKEN', 'RESEND_API_KEY',
                    'PAYPAL_CLIENT_ID', 'PAYPAL_CLIENT_SECRET', 'STRIPE_SECRET_KEY',
                    'CHECKOUT_STRIPE_WEBHOOK_SECRET', 'CHECKOUT_PAYPAL_WEBHOOK_ID', 'CHECKOUT_PAYPAL_MERCHANT_ID')
        preserved = '# keep\nDATABASE_URL=kept-db\nS3_ACCESS_KEY_ID=kept-s3\nSHIPPING_SENDER_NAME=kept-name\nOTHER="multi\nSTRIPE_SECRET_KEY=inside\nline"\n'
        for old, flag, item in (('local', '--qa', '0a291e2c-4ae4-436d-8730-ddf301a544d7'),
                                ('qa', '--local', '87323839-368f-4288-a4a5-9b0320699892')):
            with self.subTest(old=old):
                (self.cwd / '.env').write_text(preserved + f'RUN_ENV={old} # mode\n' +
                                              ''.join(f'{key}=previous-{key} # field\n' for key in imported))
                self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': 'selected-key'}])
                text = self.assert_success(self.run_setup(flag, EXPECTED_ITEM=item))
                self.assertTrue(text.startswith(preserved))
                self.assertIn(f'RUN_ENV="{flag[2:]}" # mode', text)
                self.assertIn('STRIPE_SECRET_KEY="selected-key" # field', text)
                self.assertRegex(text, r'JWT_SECRET="[0-9a-f]{64}" # field')
                self.assertRegex(text, r'CRON_SECRET="[0-9a-f]{64}" # field')
                for key in imported[2:]:
                    if key == 'STRIPE_SECRET_KEY':
                        continue
                    self.assertIn(f'{key}="" # field', text)
                self.assertNotIn('previous-', text)
                self.assertFalse((self.cwd / '.env.qa').exists())
                self.assertEqual((self.cwd / '.env').stat().st_mode & 0o777, 0o600)
                self.assertEqual(self.assert_success(self.run_setup(flag, EXPECTED_ITEM=item)), text)

    def test_switch_replaces_all_imported_values_and_duplicate_records(self):
        names = ('JWT_SECRET', 'CRON_SECRET', 'SHOPIFY_ACCESS_TOKEN', 'RESEND_API_KEY',
                 'PAYPAL_CLIENT_ID', 'PAYPAL_CLIENT_SECRET', 'STRIPE_SECRET_KEY',
                    'CHECKOUT_STRIPE_WEBHOOK_SECRET', 'CHECKOUT_PAYPAL_WEBHOOK_ID', 'CHECKOUT_PAYPAL_MERCHANT_ID')
        self.fields([{'name': name, 'value': f'selected-{name}'} for name in names])
        (self.cwd / '.env').write_text('RUN_ENV=local\nRUN_ENV=local\n' +
                                      ''.join(f'{name}="old # private" # kept\n{name}=duplicate\n' for name in names))
        text = self.assert_success(self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7'))
        self.assertEqual(text.count('RUN_ENV="qa"\n'), 2)
        for name in names:
            self.assertIn(f'{name}="selected-{name}" # kept\n{name}="selected-{name}"\n', text)
        self.assertNotIn('old # private', text)
        self.assertNotIn('duplicate', text)

    def test_missing_empty_and_unknown_existing_modes(self):
        for selector, preserved in (('', True), ('RUN_ENV=\n', True), ('RUN_ENV="" # empty\n', True),
                                     ('RUN_ENV=unknown\n', False), ('RUN_ENV="qa"\n', False)):
            with self.subTest(selector=selector):
                (self.cwd / '.env').write_text(selector + 'STRIPE_SECRET_KEY=kept\n')
                self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': 'selected'}])
                text = self.assert_success(self.run_setup('--local'))
                self.assertIn('RUN_ENV="local"', text)
                self.assertIn('STRIPE_SECRET_KEY=' + ('kept' if preserved else '"selected"'), text)

    def test_qa_missing_cli_switch_uses_defaults_then_preserves_same_mode(self):
        (self.bin / 'bw').unlink()
        (self.cwd / '.env').write_text('STRIPE_SECRET_KEY=previous\nJWT_SECRET=previous\nDATABASE_URL=kept\n')
        text = self.assert_success(self.run_setup('--qa'))
        self.assertIn('RUN_ENV="qa"', text)
        self.assertIn('STRIPE_SECRET_KEY=""', text)
        self.assertIn('DATABASE_URL=kept', text)
        self.assertNotIn('previous', text)
        self.assertEqual(self.assert_success(self.run_setup('--qa')), text)

    def test_qa_accepts_any_or_missing_provider_credential(self):
        for key in ('', 'synthetic-unclassified', 'sk_live_synthetic', 'pk_test_synthetic'):
            with self.subTest(key=key):
                (self.cwd / '.env').unlink(missing_ok=True)
                self.fields([{'name': 'STRIPE_SECRET_KEY', 'value': key}, {'name': 'JWT_SECRET', 'value': 'synthetic'}])
                text = self.assert_success(self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7'))
                self.assertIn(f'STRIPE_SECRET_KEY="{key}"', text)

    def test_qa_symlink_concurrency_and_vault_failure_preserve_shared_file(self):
        target = self.cwd / 'other'
        target.write_text('OTHER=untouched\n')
        env = self.cwd / '.env'
        env.symlink_to(target)
        self.assertNotEqual(self.run_setup('--qa').returncode, 0)
        self.assertTrue(env.is_symlink())
        self.assertEqual(target.read_text(), 'OTHER=untouched\n')
        env.unlink()
        original = 'RUN_ENV=local\nSTRIPE_SECRET_KEY=previous\n'
        env.write_text(original)
        for stage in ('status', 'get'):
            result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7', FAIL=stage)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(env.read_text(), original)
        result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7', CONCURRENT='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(env.read_text(), 'OTHER=concurrent\n')

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

    def test_checkout_credentials_fill_and_preserve_same_mode(self):
        names = ('CHECKOUT_STRIPE_WEBHOOK_SECRET', 'CHECKOUT_PAYPAL_WEBHOOK_ID', 'CHECKOUT_PAYPAL_MERCHANT_ID')
        self.fields([{'name': name, 'value': f'vault-{name}', 'type': 1} for name in names])
        for flag, item in (('--local', '87323839-368f-4288-a4a5-9b0320699892'),
                           ('--qa', '0a291e2c-4ae4-436d-8730-ddf301a544d7')):
            for initial in ('', 'empty', 'kept'):
                with self.subTest(flag=flag, initial=initial):
                    records = '' if not initial else ''.join(f'{name}={"" if initial == "empty" else "kept"}\n' for name in names)
                    (self.cwd / '.env').write_text(f'RUN_ENV={flag[2:]}\n' + records)
                    result = self.run_setup(flag, EXPECTED_ITEM=item)
                    text = self.assert_success(result)
                    for name in names:
                        self.assertIn(f'{name}=' + ('kept' if initial == 'kept' else f'"vault-{name}"'), text)
                        self.assertNotIn(f'vault-{name}', result.stdout + result.stderr)

    def test_checkout_credentials_reject_ambiguous_or_invalid_fields_privately(self):
        for name in ('CHECKOUT_STRIPE_WEBHOOK_SECRET', 'CHECKOUT_PAYPAL_WEBHOOK_ID', 'CHECKOUT_PAYPAL_MERCHANT_ID'):
            for fields in ([{'name': name, 'value': 'private-a'}, {'name': name, 'value': 'private-b'}],
                           [{'name': name, 'value': 'private-value', 'type': 3}],
                           [{'name': name, 'value': 42}],
                           [{'name': name, 'value': 'private\rvalue'}]):
                with self.subTest(name=name, fields=fields):
                    original = 'RUN_ENV=qa\nOTHER=kept\n'
                    (self.cwd / '.env').write_text(original)
                    self.fields([{'name': 'JWT_SECRET', 'value': 'private-signing'}, *fields])
                    result = self.run_setup('--qa', EXPECTED_ITEM='0a291e2c-4ae4-436d-8730-ddf301a544d7')
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual((self.cwd / '.env').read_text(), original)
                    self.assertNotIn('private-', result.stdout + result.stderr)

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
        self.assertIn('RUN_ENV="local"', text)
        for name in excluded[1:]:
            self.assertIn(f'{name}="existing-value"', text)

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

    def prepare_just_setup(self):
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
        cargo.write_text(r"""#!/usr/bin/env python3
import json, sys
from pathlib import Path
args = sys.argv[1:]
binary = Path('.tools/bin/dprint')
if binary.exists() and '--force' not in args:
    print('error: binary `dprint` already exists in destination', file=sys.stderr)
    sys.exit(101)
Path('cargo-ran').write_text(json.dumps(args))
binary.parent.mkdir(parents=True, exist_ok=True)
binary.write_text("#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'dprint 0.63.0'; else : > formatter-configured; fi\n")
binary.chmod(0o755)
""")
        cargo.chmod(0o755)
        return just, api

    def test_setup_reuses_pinned_formatter_and_repairs_other_states(self):
        just, api = self.prepare_just_setup()
        binary = api / '.tools/bin/dprint'
        binary.parent.mkdir(parents=True)
        cases = (('missing', None, True), ('pinned', "echo 'dprint 0.63.0'", False),
                 ('outdated', "echo 'dprint 0.62.0'", True), ('unusable', 'exit 1', True),
                 ('failed-version', "echo 'dprint 0.63.0'; exit 1", True),
                 ('not-executable', "echo 'dprint 0.63.0'", True))
        for state, version_command, install in cases:
            with self.subTest(state=state):
                binary.unlink(missing_ok=True)
                (api / 'cargo-ran').unlink(missing_ok=True)
                (api / 'formatter-configured').unlink(missing_ok=True)
                (api / '.env').unlink(missing_ok=True)
                if version_command:
                    binary.write_text('#!/bin/sh\nif [ "$1" = --version ]; then ' + version_command +
                                      '; else : > formatter-configured; fi\n')
                    binary.chmod(0o644 if state == 'not-executable' else 0o755)
                result = subprocess.run([just, '--justfile', str(api / 'justfile'), 'setup', '--qa'],
                                        cwd=api, env=self.env | {'EXPECTED_ITEM': '0a291e2c-4ae4-436d-8730-ddf301a544d7'},
                                        stdin=subprocess.DEVNULL, capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('RUN_ENV="qa"', (api / '.env').read_text())
                self.assertIn('SHOPIFY_ACCESS_TOKEN="synthetic-vault-token"', (api / '.env').read_text())
                self.assertTrue((api / 'formatter-configured').exists())
                self.assertEqual((api / 'cargo-ran').exists(), install)
                if install:
                    self.assertEqual(json.loads((api / 'cargo-ran').read_text()),
                                     ['install', '--force', '--locked', 'dprint', '--version', '0.63.0', '--root', '.tools'])

    def test_single_just_setup_works_with_present_or_absent_cli(self):
        just, api = self.prepare_just_setup()
        for args, present in (((), True), (('--local',), True), (('--qa',), True), ((), False)):
            with self.subTest(args=args, cli_present=present):
                if not present:
                    (self.bin / 'bw').unlink()
                qa = args == ('--qa',)
                if qa:
                    self.fields([{'name': 'SHOPIFY_ACCESS_TOKEN', 'value': 'synthetic-vault-token'},
                                 {'name': 'STRIPE_SECRET_KEY', 'value': 'sk_test_synthetic'}])
                selected = api / '.env'
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

    def test_noarg_run_loads_shared_dotenv_for_migrations_and_api(self):
        for mode in ('local', 'qa'):
            with self.subTest(mode=mode):
                (self.root / 'events.jsonl').unlink(missing_ok=True)
                (self.api / '.docker-started').unlink(missing_ok=True)
                (self.api / '.env').write_text(f'RUN_ENV={mode}\nDATABASE_URL=file-db\nSTRIPE_SECRET_KEY=arbitrary-provider-key\n')
                result = self.run_recipe('run')
                self.assertEqual(result.returncode, 0, result.stderr)
                consumers = [event for event in self.events() if event['command'] in ('cargo', 'api')]
                self.assertEqual([event['args'] for event in consumers[:2]],
                                 [['sqlx', '--no-dotenv', 'migrate', 'run'], ['build', '--locked', '-p', 'api', '--bin', 'api']])
                self.assertTrue(any(event['command'] == 'api' for event in consumers))
                for event in consumers:
                    self.assertEqual(event['env']['RUN_ENV'], mode)
                    self.assertEqual(event['env']['DATABASE_URL'], 'file-db')
                    self.assertEqual(event['env']['STRIPE_SECRET_KEY'], 'arbitrary-provider-key')
                self.assertEqual(self.events()[-1]['args'], ['compose', '-f', 'docker-compose.yml', 'stop', 'local-s3'])

    def test_inherited_precedence_and_unowned_s3_preserved(self):
        (self.api / '.env').write_text('RUN_ENV=local\nDATABASE_URL=file-db\nSTRIPE_SECRET_KEY=file-key\n')
        result = self.run_recipe('run', RUN_ENV='qa', DATABASE_URL='inherited-db', STRIPE_SECRET_KEY='inherited-key', S3_EXISTS='1')
        self.assertEqual(result.returncode, 0, result.stderr)
        for event in self.events():
            if event['command'] in ('cargo', 'api'):
                self.assertEqual(event['env']['RUN_ENV'], 'qa')
                self.assertEqual(event['env']['DATABASE_URL'], 'inherited-db')
                self.assertEqual(event['env']['STRIPE_SECRET_KEY'], 'inherited-key')
        self.assertFalse(any(event['args'][-2:] == ['stop', 'local-s3'] for event in self.events()))

    def test_no_run_mode_or_credential_preflight(self):
        result = self.run_recipe('run')
        self.assertEqual(result.returncode, 0, result.stderr)
        for event in self.events():
            if event['command'] in ('cargo', 'api'):
                self.assertNotIn('RUN_ENV', event['env'])
                self.assertNotIn('STRIPE_SECRET_KEY', event['env'])

    def test_invalid_recipe_flags_fail_before_setup_or_docker(self):
        for recipe in ('setup', 'run'):
            invalid = [('local',), ('--bad',), ('--qa', '--local'), ('--local', 'excess')]
            if recipe == 'run':
                invalid.extend([('--qa',), ('--local',)])
            for args in invalid:
                with self.subTest(recipe=recipe, args=args):
                    result = self.run_recipe(recipe, *args)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(self.events(), "A dependency command ran before validation")
                    self.assertFalse((self.api / '.env').exists())
                    self.assertFalse((self.api / '.env.qa').exists())

    def test_failed_migration_runs_owned_s3_cleanup(self):
        (self.api / '.env').write_text('RUN_ENV=qa\nDATABASE_URL=qa-db\n')
        result = self.run_recipe('run', CARGO_FAIL='1')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.events(), result.stderr)
        self.assertEqual(self.events()[-1]['args'], ['compose', '-f', 'docker-compose.yml', 'stop', 'local-s3'])
        self.assertFalse(any(event['command'] == 'api' for event in self.events()))


if __name__ == '__main__':
    unittest.main()
