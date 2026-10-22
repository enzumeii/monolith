import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile, rm, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
process.chdir(root);

let activeChild;
let stoppedBy;
function stop(signal) {
  stoppedBy = signal;
  if (!activeChild?.pid) return;
  try {
    if (process.platform === 'win32') activeChild.kill(signal);
    else process.kill(-activeChild.pid, signal);
  } catch (error) {
    if (error.code !== 'ESRCH') throw error;
  }
}
const interrupt = () => stop('SIGINT');
const terminate = () => stop('SIGTERM');
process.on('SIGINT', interrupt);
process.on('SIGTERM', terminate);

async function run(command, args) {
  if (stoppedBy) return stoppedBy === 'SIGINT' ? 130 : 143;
  const child = spawn(command, args, {
    stdio: 'inherit', env: process.env, detached: process.platform !== 'win32',
  });
  activeChild = child;
  try {
    return await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => resolve(code ?? (signal === 'SIGINT' ? 130 : 143)));
    });
  } finally {
    activeChild = undefined;
  }
}

let configPath;
try {
  for (const envPath of ['.env', 'services/api/.env']) {
    try {
      process.loadEnvFile(envPath);
    } catch (error) {
      if (error.code !== 'ENOENT') throw new Error(`Cannot read ${envPath}`);
    }
  }
  const missing = ['DATABASE_URL', 'JWT_SECRET', 'CRON_SECRET'].filter(key => !process.env[key]?.trim());
  if (missing.length) throw new Error(`Missing local API settings: ${missing.join(', ')}. Run just setup in services/api.`);
  process.env.RUN_ENV = 'local';
  process.env.VERCEL_ENV = 'development';
  process.env.VERCEL_URL = 'localhost:18027';

  const config = JSON.parse(await readFile('vercel.json', 'utf8'));
  config.services.api = { root: '.', devCommand: config.services.api.devCommand };
  if (!config.services.api.devCommand) throw new Error('API local devCommand is required');
  configPath = `.vercel-dev-${randomUUID()}.json`;
  await writeFile(configPath, JSON.stringify(config), { mode: 0o600, flag: 'wx' });
  for (const [app, prefix] of [['backoffice', 'backoffice'], ['client-ui', 'user']]) {
    const code = await run('bash', ['scripts/build-vercel-ui.sh', app, prefix]);
    if (code) {
      process.exitCode = code;
      break;
    }
  }
  if (!process.exitCode) {
    process.exitCode = await run(process.execPath, [
      'node_modules/vercel/dist/index.js', 'dev', '--local', '--local-config', configPath,
      '--listen', '18027', '--queue-port', '55461', '--runtime-cache-port', '55462', '--schedules-port', '55463',
    ]);
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
} finally {
  if (configPath) await rm(configPath, { force: true });
  process.removeListener('SIGINT', interrupt);
  process.removeListener('SIGTERM', terminate);
}
