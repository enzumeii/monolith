const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const test = require('node:test');
const { parse } = require('smol-toml');

for (const mode of ['build', 'serve']) {
  for (const exitCode of [0, 42]) {
    test(`client ${mode} composes local settings and restores baseline after exit ${exitCode}`, t => {
      const root = fs.mkdtempSync(path.join(os.tmpdir(), 'client-public-config-'));
      t.after(() => fs.rmSync(root, { recursive: true, force: true }));
      fs.mkdirSync(path.join(root, 'scripts'));
      fs.mkdirSync(path.join(root, 'apps/client-ui'), { recursive: true });
      fs.mkdirSync(path.join(root, 'bin'));
      for (const script of ['run-client-ui.sh', 'compose-vercel-ui-config.mjs']) {
        fs.copyFileSync(`scripts/${script}`, path.join(root, 'scripts', script));
      }
      fs.symlinkSync(path.resolve('node_modules'), path.join(root, 'node_modules'), 'dir');
      for (const file of ['application.toml', 'application.local.toml']) {
        fs.copyFileSync(`apps/client-ui/${file}`, path.join(root, 'apps/client-ui', file));
      }
      const original = fs.readFileSync(path.join(root, 'apps/client-ui/application.toml'), 'utf8');
      fs.writeFileSync(path.join(root, 'bin/dx'), '#!/bin/sh\ncp application.toml selected.toml\nprintf "%s\\n" "$@" > arguments.txt\nexit "$DX_EXIT"\n', { mode: 0o755 });
      const result = spawnSync('bash', ['scripts/run-client-ui.sh', mode], {
        cwd: root, encoding: 'utf8', env: { ...process.env, PATH: `${path.join(root, 'bin')}:${process.env.PATH}`, DX_EXIT: String(exitCode) },
      });
      assert.equal(result.status, exitCode, result.stderr);
      assert.equal(fs.readFileSync(path.join(root, 'apps/client-ui/application.toml'), 'utf8'), original);
      const selected = parse(fs.readFileSync(path.join(root, 'apps/client-ui/selected.toml'), 'utf8'));
      assert.equal(selected.api.base_url, 'http://localhost:3000/v1');
      assert.equal(selected.visitor.country_lookup_url, 'https://api.country.is/');
      assert.equal(selected.contact.email, parse(original).contact.email);
      const args = fs.readFileSync(path.join(root, 'apps/client-ui/arguments.txt'), 'utf8').trim().split('\n');
      assert.deepEqual(args, mode === 'build' ? ['build', '--web', '--package', 'client-ui'] : ['serve', '--web', '--addr', '127.0.0.1', '--port', '8085']);
    });
  }
}
