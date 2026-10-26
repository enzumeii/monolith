const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');
const { once } = require('node:events');
const test = require('node:test');

function fixture(t, app) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'frontend-local-config-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const directory of ['scripts', `apps/${app}`, 'bin']) fs.mkdirSync(path.join(root, directory), { recursive: true });
  fs.copyFileSync('scripts/run-ui.sh', path.join(root, 'scripts/run-ui.sh'));
  const source = path.join(root, `apps/${app}/application.toml`);
  fs.copyFileSync(`apps/${app}/application.toml`, source);
  const original = fs.readFileSync(source, 'utf8');
  fs.writeFileSync(path.join(root, 'bin/dx'), `#!/bin/sh
cp application.toml observed-baseline.toml
printf '%s\\n' "$@" > arguments.txt
printf '%s\\n' "$RUN_ENV" > selected-environment.txt
if [ "\${DX_WAIT:-0}" = 1 ]; then printf 'ready\\n'; while :; do sleep 1; done; fi
exit "$DX_EXIT"
`, { mode: 0o755 });
  const env = { ...process.env, PATH: `${path.join(root, 'bin')}:${process.env.PATH}`, DX_EXIT: '0' };
  delete env.RUN_ENV;
  return { root, source, original, env, appRoot: path.dirname(source) };
}

for (const [app, port] of [['client-ui', '8085'], ['backoffice', '8084']]) {
  for (const mode of ['build', 'serve']) {
    for (const selection of [
      { name: 'default', expected: 'local' },
      { name: 'RUN_ENV', runEnv: 'qa', expected: 'qa' },
      { name: 'explicit argument', runEnv: 'qa', argument: 'prod', expected: 'prod' },
    ]) {
      for (const exitCode of [0, 42]) {
        test(`${app} ${mode} selects ${selection.name} without rewriting source on exit ${exitCode}`, t => {
          const f = fixture(t, app);
          const env = { ...f.env, DX_EXIT: String(exitCode) };
          if (selection.runEnv) env.RUN_ENV = selection.runEnv;
          const result = spawnSync('bash', ['scripts/run-ui.sh', app, mode, ...(selection.argument ? [selection.argument] : [])], {
            cwd: f.root, encoding: 'utf8', env,
          });
          assert.equal(result.status, exitCode, result.stderr);
          assert.equal(fs.readFileSync(f.source, 'utf8'), f.original);
          assert.equal(fs.readFileSync(path.join(f.appRoot, 'observed-baseline.toml'), 'utf8'), f.original);
          assert.equal(fs.readFileSync(path.join(f.appRoot, 'selected-environment.txt'), 'utf8').trim(), selection.expected);
          const args = fs.readFileSync(path.join(f.appRoot, 'arguments.txt'), 'utf8').trim().split('\n');
          assert.deepEqual(args, mode === 'build' ? ['build', '--web', '--package', app] : ['serve', '--web', '--addr', '127.0.0.1', '--port', port]);
        });
      }
    }
  }
  test(`${app} serve interruption leaves production source untouched`, { timeout: 30000 }, async t => {
    const f = fixture(t, app);
    const child = spawn('bash', ['scripts/run-ui.sh', app, 'serve'], {
      cwd: f.root, env: { ...f.env, DX_WAIT: '1' }, detached: true, stdio: ['ignore', 'pipe', 'ignore'],
    });
    t.after(() => { try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; } });
    const exited = new Promise((resolve, reject) => { child.once('exit', (code, signal) => resolve({ code, signal })); child.once('error', reject); });
    const [ready] = await Promise.race([
      once(child.stdout, 'data'),
      exited.then(result => { throw new Error(`fake dx exited before readiness: ${JSON.stringify(result)}`); }),
    ]);
    assert.match(ready.toString(), /ready/);
    process.kill(-child.pid, 'SIGTERM');
    const result = await exited;
    assert.ok(result.code === 143 || result.signal === 'SIGTERM', JSON.stringify(result));
    assert.equal(fs.readFileSync(f.source, 'utf8'), f.original);
  });
}
