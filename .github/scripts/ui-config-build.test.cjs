const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { after, test } = require('node:test');
const { setTimeout: delay } = require('node:timers/promises');
const { parse } = require('smol-toml');

const compilerRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'ui-config-build-compiler-'));
after(() => fs.rmSync(compilerRoot, { recursive: true, force: true }));
let executable;
function buildScript() {
  if (executable) return executable;
  executable = path.join(compilerRoot, 'ui-config-build');
  const compiled = spawnSync('rustc', ['--edition=2024', 'scripts/ui-config-build.rs', '-o', executable], { encoding: 'utf8' });
  assert.equal(compiled.status, 0, compiled.stderr);
  return executable;
}
function fixture(t, app) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ui-config-build-inputs-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const appRoot = path.join(root, 'apps', app);
  fs.mkdirSync(appRoot, { recursive: true });
  fs.mkdirSync(path.join(root, 'scripts'));
  fs.copyFileSync('scripts/compose-vercel-ui-config.mjs', path.join(root, 'scripts/compose-vercel-ui-config.mjs'));
  fs.symlinkSync(path.resolve('node_modules'), path.join(root, 'node_modules'), 'dir');
  const baseline = '[api]\nbase_url="/api/v1"\ntimeout=10\n[contact]\nemail="orders@example.test"\n[display]\ncolumns=[1,2]\n';
  fs.writeFileSync(path.join(appRoot, 'application.toml'), baseline);
  fs.writeFileSync(path.join(appRoot, 'application.local.toml'), '[api]\nbase_url="http://localhost:3000/v1"\n');
  const env = { ...process.env, CARGO_MANIFEST_DIR: appRoot, PROFILE: 'debug' };
  delete env.RUN_ENV;
  delete env.UI_CONFIG_OVERRIDE;
  function run(selected = {}) {
    return spawnSync(buildScript(), [], { cwd: root, env: { ...env, ...selected }, encoding: 'utf8' });
  }
  function output(result) {
    assert.equal(result.status, 0, result.stderr);
    const match = result.stdout.match(/^cargo:rustc-env=PUBLIC_CONFIG_ASSET=(.+)$/m);
    assert.ok(match, result.stdout);
    const generated = path.join(appRoot, match[1]);
    assert.equal(path.basename(generated), 'application.toml');
    assert.equal(fs.readFileSync(path.join(appRoot, 'application.toml'), 'utf8'), baseline);
    return { generated, contents: parse(fs.readFileSync(generated, 'utf8')) };
  }
  return { root, appRoot, baseline, run, output };
}
for (const app of ['client-ui', 'backoffice']) {
  test(`${app} debug composes local overrides while inheriting all baseline tables`, t => {
    const f = fixture(t, app);
    const { generated, contents } = f.output(f.run());
    assert.match(generated, /target\/public-config\/local\/[a-f0-9]{16}\/application.toml$/);
    assert.equal(contents.api.base_url, 'http://localhost:3000/v1');
    assert.equal(contents.api.timeout, 10);
    assert.equal(contents.contact.email, 'orders@example.test');
    assert.deepEqual(contents.display.columns, [1, 2]);
  });
  test(`${app} release inherits production without requiring a redundant overlay`, t => {
    const f = fixture(t, app);
    const { generated, contents } = f.output(f.run({ PROFILE: 'release' }));
    assert.match(generated, /target\/public-config\/prod\/[a-f0-9]{16}\/application.toml$/);
    assert.deepEqual(contents, parse(f.baseline));
  });
  test(`${app} existing production overrides replace only their selected settings`, t => {
    const f = fixture(t, app);
    const overlay = '[api]\nbase_url="https://production.example.test/api/v1"\n';
    fs.writeFileSync(path.join(f.appRoot, 'application.prod.toml'), overlay);
    const { contents } = f.output(f.run({ RUN_ENV: 'prod' }));
    assert.equal(contents.api.base_url, 'https://production.example.test/api/v1');
    assert.equal(contents.api.timeout, 10);
    assert.equal(contents.contact.email, 'orders@example.test');
    assert.equal(fs.readFileSync(path.join(f.appRoot, 'application.prod.toml'), 'utf8'), overlay);
  });
  test(`${app} RUN_ENV selects the requested overlay independently of build profile`, t => {
    const f = fixture(t, app);
    fs.writeFileSync(path.join(f.appRoot, 'application.qa.toml'), '[api]\nbase_url="https://qa.example.test/api/v1"\n[display]\ncolumns=[3]\n');
    const { generated, contents } = f.output(f.run({ RUN_ENV: 'qa', PROFILE: 'release' }));
    assert.match(generated, /target\/public-config\/qa\/[a-f0-9]{16}\/application.toml$/);
    assert.equal(contents.api.base_url, 'https://qa.example.test/api/v1');
    assert.equal(contents.api.timeout, 10);
    assert.deepEqual(contents.display.columns, [3]);
  });
  test(`${app} simultaneous production selections retain distinct generated assets`, t => {
    const f = fixture(t, app);
    const production = f.output(f.run({ RUN_ENV: 'prod' }));
    const original = fs.readFileSync(production.generated, 'utf8');
    fs.writeFileSync(path.join(f.appRoot, 'preview.toml'), '[api]\nbase_url="https://preview.example.test/api/v1"\n');
    const preview = f.output(f.run({ RUN_ENV: 'prod', UI_CONFIG_OVERRIDE: 'preview.toml' }));
    assert.notEqual(preview.generated, production.generated, 'a second build must not replace an asset still used by the first build');
    assert.equal(fs.readFileSync(production.generated, 'utf8'), original);
    assert.equal(parse(original).api.base_url, '/api/v1');
    assert.equal(preview.contents.api.base_url, 'https://preview.example.test/api/v1');
  });
  test(`${app} changed local settings retain the asset used by an earlier build`, t => {
    const f = fixture(t, app);
    const initial = f.output(f.run());
    const original = fs.readFileSync(initial.generated, 'utf8');
    fs.writeFileSync(path.join(f.appRoot, 'application.local.toml'), '[api]\nbase_url="http://localhost:3002/v1"\n');
    const updated = f.output(f.run());
    assert.notEqual(updated.generated, initial.generated);
    assert.equal(fs.readFileSync(initial.generated, 'utf8'), original);
    assert.equal(initial.contents.api.base_url, 'http://localhost:3000/v1');
    assert.equal(updated.contents.api.base_url, 'http://localhost:3002/v1');
  });
  test(`${app} explicit packaging override remains supported without modifying its source`, t => {
    const f = fixture(t, app);
    const alternate = '[api]\nbase_url="https://preview.example.test/api/v1"\n';
    fs.writeFileSync(path.join(f.appRoot, 'preview.toml'), alternate);
    const { contents } = f.output(f.run({ RUN_ENV: 'prod', UI_CONFIG_OVERRIDE: 'preview.toml' }));
    assert.equal(contents.api.base_url, 'https://preview.example.test/api/v1');
    assert.equal(fs.readFileSync(path.join(f.appRoot, 'preview.toml'), 'utf8'), alternate);
  });
  for (const [name, selected, filename, contents] of [
    ['missing local', {}, 'application.local.toml', null],
    ['missing requested QA', { RUN_ENV: 'qa' }, null, null],
    ['missing explicit override', { RUN_ENV: 'prod', UI_CONFIG_OVERRIDE: 'missing.toml' }, null, null],
    ['malformed local', {}, 'application.local.toml', '[invalid'],
    ['malformed optional prod', { RUN_ENV: 'prod' }, 'application.prod.toml', '[invalid'],
    ['unsafe environment', { RUN_ENV: '../local' }, null, null],
  ]) {
    test(`${app} rejects ${name} without changing the baseline`, t => {
      const f = fixture(t, app);
      if (filename) {
        if (contents === null) fs.unlinkSync(path.join(f.appRoot, filename));
        else fs.writeFileSync(path.join(f.appRoot, filename), contents);
      }
      const result = f.run(selected);
      assert.notEqual(result.status, 0, result.stdout);
      assert.equal(fs.readFileSync(path.join(f.appRoot, 'application.toml'), 'utf8'), f.baseline);
      assert.doesNotMatch(result.stdout, /^cargo:rustc-env=PUBLIC_CONFIG_ASSET=/m);
    });
  }
  test(`${app} unchanged composition preserves the generated asset modification time`, async t => {
    const f = fixture(t, app);
    const { generated } = f.output(f.run());
    const before = fs.statSync(generated, { bigint: true }).mtimeNs;
    await delay(30);
    f.output(f.run());
    assert.equal(fs.statSync(generated, { bigint: true }).mtimeNs, before);
  });
}
