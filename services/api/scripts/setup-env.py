#!/usr/bin/env python3
"""Fill private API settings from Bitwarden when installed, or local defaults."""

import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import tempfile


ITEM_ID = '87323839-368f-4288-a4a5-9b0320699892'
KEYS = (
    'JWT_SECRET', 'SHOPIFY_ACCESS_TOKEN', 'RESEND_API_KEY',
    'PAYPAL_CLIENT_ID', 'PAYPAL_CLIENT_SECRET', 'STRIPE_SECRET_KEY',
)
ASSIGNMENT = re.compile(r'^[ \t]*(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_.]*)[ \t]*=(.*)', re.S)
EMPTY = re.compile(r'''^[ \t]*(?:""|'')?[ \t]*(?:#.*)?$''')


class SetupError(Exception):
    pass


def main():
    if len(sys.argv) != 1:
        raise SetupError('Usage: setup-env.py')
    path = Path('.env')
    original = snapshot(path)
    values = vault_values()
    defaults = local_defaults()
    defaults.update({key: value for key, value in values.items() if value})
    text = merge((original[1] if original else b'').decode('utf-8'), defaults)
    write_private(path, original, text.encode('utf-8'))
    if values:
        print('API .env prepared from matching Bitwarden fields; existing nonempty values preserved.')
    else:
        print('API .env prepared with local defaults; existing nonempty values preserved.')


def vault_values():
    bw = shutil.which('bw')
    if bw is None:
        return {}
    child_env = os.environ.copy()
    status = read_json(run_bw(bw, ['status', '--nointeraction'], child_env, 'status'), 'status')
    state = status.get('status') if isinstance(status, dict) else None
    if state == 'unauthenticated':
        raise SetupError('Bitwarden CLI is not logged in. Run bw login, then rerun just setup.')
    if state == 'locked':
        if not sys.stdin.isatty() or not sys.stderr.isatty():
            raise SetupError('Bitwarden is locked. Rerun just setup in an interactive terminal to unlock, '
                             'or supply an unlocked BW_SESSION in your terminal environment.')
        child_env.pop('BW_SESSION', None)
        child_env.pop('BW_NOINTERACTION', None)
        session = run_bw(bw, ['unlock', '--raw'], child_env, 'unlock', interactive=True).strip()
        if not session or any(character.isspace() for character in session):
            raise SetupError('Bitwarden unlock did not return a valid session. Rerun just setup.')
        child_env['BW_SESSION'] = session
    elif state != 'unlocked':
        raise SetupError('Bitwarden returned an invalid status. Check bw status in your terminal.')
    item = read_json(run_bw(bw, ['get', 'item', ITEM_ID, '--nointeraction'], child_env, 'item retrieval'), 'item')
    fields = item.get('fields') if isinstance(item, dict) else None
    if not isinstance(fields, list):
        raise SetupError('Bitwarden item must contain custom fields named after API environment settings.')
    values = {}
    for field in fields:
        if not isinstance(field, dict):
            raise SetupError('Bitwarden item has malformed custom fields.')
        name = field.get('name')
        if not isinstance(name, str) or name not in KEYS:
            continue
        value = field.get('value')
        if name in values or field.get('type', 0) not in (0, 1) or not isinstance(value, (str, type(None))):
            raise SetupError('Bitwarden item has duplicate or unsupported matching custom fields.')
        if value is not None and ('\x00' in value or '\r' in value):
            raise SetupError('Bitwarden field contains unsupported NUL or carriage-return characters.')
        values[name] = value or ''
    if not any(values.values()):
        raise SetupError('Bitwarden item has no nonempty matching API custom fields. Check its field names.')
    return values


def run_bw(bw, args, child_env, stage, interactive=False):
    # Only unlock owns terminal input/error output, so Bitwarden can hide the password itself.
    result = subprocess.run([bw, *args], env=child_env, text=True, stdout=subprocess.PIPE,
                            stdin=None if interactive else subprocess.DEVNULL,
                            stderr=None if interactive else subprocess.DEVNULL)
    if result.returncode != 0:
        raise SetupError(f'Bitwarden {stage} failed or was cancelled. Check the CLI in your terminal and rerun just setup.')
    return result.stdout


def read_json(text, stage):
    try:
        return json.loads(text)
    except (ValueError, TypeError):
        raise SetupError(f'Bitwarden returned invalid {stage} data.') from None


def local_defaults():
    token = secrets.token_hex(4)
    return {
        'RUN_ENV': 'local',
        'DATABASE_URL': 'postgres://user:pass@127.0.0.1:5432/core',
        'JWT_SECRET': secrets.token_hex(32),
        'CRON_SECRET': secrets.token_hex(32),
        'SHOPIFY_ACCESS_TOKEN': '', 'RESEND_API_KEY': '',
        'PAYPAL_CLIENT_ID': '', 'PAYPAL_CLIENT_SECRET': '', 'STRIPE_SECRET_KEY': '',
        'BLOB_READ_WRITE_TOKEN': 'vercel_blob_rw_local_synthetic',
        'SHIPPING_SENDER_NAME': f'Synthetic Sender {token}',
        'SHIPPING_SENDER_PHONE': f'000{secrets.randbelow(10_000_000):07d}',
        'SHIPPING_SENDER_EMAIL': 'shipping@enzumeii.com',
        'SHIPPING_SENDER_ADDRESS': f'{secrets.randbelow(9999) + 1} Synthetic Street {token}',
        'SHIPPING_SENDER_SUB_DISTRICT': 'Synthetic Subdistrict',
        'SHIPPING_SENDER_DISTRICT': 'Synthetic District',
        'SHIPPING_SENDER_PROVINCE': 'Bangkok',
        'SHIPPING_SENDER_POSTCODE': f'10{secrets.randbelow(1000):03d}',
        'SHIPPING_SENDER_COUNTRY_CODE': 'TH',
    }


def records(text):
    """Keep multiline quoted values together so their contents cannot become assignments."""
    start = 0
    quote = None
    escaped = False
    comment = False
    for index, char in enumerate(text):
        if char == '\n' and quote is None:
            yield text[start:index + 1]
            start = index + 1
            escaped = comment = False
        elif comment:
            continue
        elif escaped:
            escaped = False
        elif char == '\\' and quote != "'":
            escaped = True
        elif char == quote:
            quote = None
        elif quote is None:
            if char in ('"', "'"):
                quote = char
            elif char == '#':
                prefix = text[start:index]
                assignment = ASSIGNMENT.match(prefix)
                if not prefix.strip() or text[index - 1].isspace() or (assignment and not assignment.group(2).strip()):
                    comment = True
    if quote:
        raise SetupError('Existing .env contains an unfinished quoted value; fix it before setup.')
    if start < len(text):
        yield text[start:]


def encode(value):
    # dotenvy accepts these escapes; JSON's \r and \t escapes are not supported.
    return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('$', '\\$').replace('\n', '\\n') + '"'


def merge(text, defaults):
    entries = []
    existing = {}
    for record in records(text):
        match = ASSIGNMENT.match(record.rstrip('\r\n'))
        key, value = match.groups() if match else (None, None)
        entries.append((record, key, value))
        if key in defaults and not EMPTY.fullmatch(value):
            existing.setdefault(key, value)
    seen = set()
    output = []
    for record, key, value in entries:
        if key in defaults:
            seen.add(key)
            if EMPTY.fullmatch(value):
                # dotenvy uses the first assignment, including an empty one.
                replacement = existing.get(key, encode(defaults[key]))
                comment = value[value.index('#'):] if '#' in value else ''
                ending = '\r\n' if record.endswith('\r\n') else '\n' if record.endswith('\n') else ''
                record = f'{key}={replacement}' + (' ' + comment if comment else '') + ending
        output.append(record)
    result = ''.join(output)
    for key, value in defaults.items():
        if key not in seen:
            if result and not result.endswith('\n'):
                result += '\n'
            result += f'{key}={encode(value)}\n'
    return result


def snapshot(path):
    if path.is_symlink():
        raise SetupError('Refusing to replace a symlink at .env; use a regular private file.')
    try:
        info = path.stat()
        return ((info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns), path.read_bytes())
    except FileNotFoundError:
        return None


def write_private(path, original, content):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix='.env.', dir=path.parent, delete=False) as staged:
            temporary = Path(staged.name)
            os.fchmod(staged.fileno(), 0o600)
            staged.write(content)
            staged.flush()
            os.fsync(staged.fileno())
        if snapshot(path) != original:
            raise SetupError('.env changed during setup. Rerun setup to preserve the latest values.')
        if original is None:
            os.link(temporary, path)
        else:
            os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


if __name__ == '__main__':
    try:
        main()
    except SetupError as error:
        print(f'Setup: {error}', file=sys.stderr)
        sys.exit(1)
    except (OSError, UnicodeError):
        print('Setup: unable to read CLI data or safely update .env. Check permissions and rerun setup.', file=sys.stderr)
        sys.exit(1)
    except KeyboardInterrupt:
        print('\nSetup cancelled; .env was not updated.', file=sys.stderr)
        sys.exit(130)
