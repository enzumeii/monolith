import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { validateConfig } from '../node_modules/vercel/dist/chunks/chunk-VPH6RJ5P.js';
import { require_dist2 } from '../node_modules/vercel/dist/chunks/chunk-CVDK3IYT.js';

const config = JSON.parse(await readFile(new URL('../vercel.json', import.meta.url)));
const packageJson = JSON.parse(await readFile(new URL('../package.json', import.meta.url)));
assert.equal(packageJson.devDependencies.vercel, '65.1.0');
assert.equal(validateConfig(config), null, 'Pinned Vercel CLI configuration validation');
const { LocalFileSystemDetector, resolveAllConfiguredServicesV2 } = require_dist2();
const resolved = await resolveAllConfiguredServicesV2(config.services, new LocalFileSystemDetector(process.cwd()));
assert.deepEqual(resolved.errors, []);
assert.deepEqual(Object.keys(config.services), ['backoffice', 'client-ui', 'api']);
for (const [name, prefix] of [['backoffice', 'backoffice'], ['client-ui', 'user']]) {
  const buildCommand = `bash ../../../scripts/build-vercel-ui.sh ${name} ${prefix}`;
  assert.deepEqual(config.services[name], {
    root: `apps/${name}/vercel`, installCommand: 'true', buildCommand, outputDirectory: 'public',
    devCommand: `node ../../../scripts/serve-vercel-ui.mjs ${name}`,
    routes: [
      { handle: 'filesystem' },
      { src: `/${prefix}/assets/.*`, status: 404 },
      { src: `/${prefix}(?:/.*)?`, dest: `/${prefix}/index.html` },
    ],
  });
  const metadata = JSON.parse(await readFile(`apps/${name}/vercel/package.json`, 'utf8'));
  assert.equal(metadata.private, true);
  assert.deepEqual(metadata.scripts, { build: buildCommand });
  assert.ok(!metadata.dependencies && !metadata.devDependencies);
  assert.equal(resolved.services.find(service => service.name === name).builder.use, '@vercel/static-build');
}
assert.deepEqual(config.services.api, { root: '.', runtime: 'container', entrypoint: 'Dockerfile.api.vercel' });
assert.equal(resolved.services.find(service => service.name === 'api').builder.use, '@vercel/container');
for (const service of Object.values(config.services)) assert.ok(!('bindings' in service));
const dockerfile = await readFile('Dockerfile.api.vercel', 'utf8');
assert.match(dockerfile, /USER app/);
assert.ok(dockerfile.includes("COPY services/api/application.toml services/api/application.prod.toml /app/"));
assert.ok(dockerfile.includes("RUN_ENV=prod"));
assert.ok(dockerfile.includes('CMD ["/app/api", "server"]'));
assert.deepEqual(config.rewrites, [
  { source: '/_cron', destination: { service: 'api' } },
  { source: '/api/(.*)', destination: { service: 'api' } },
  { source: '/backoffice', destination: { service: 'backoffice' } },
  { source: '/backoffice/(.*)', destination: { service: 'backoffice' } },
  { source: '/user', destination: { service: 'client-ui' } },
  { source: '/user/(.*)', destination: { service: 'client-ui' } },
]);
assert.deepEqual(config.crons, [{ path: '/_cron', schedule: '0 0 * * *' }]);
console.log('Vercel 65.1.0 parsed two static services and one API container; public API, local SPA routes, zero bindings and daily cron match.');
