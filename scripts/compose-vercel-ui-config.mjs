import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { createHash } from 'node:crypto';
import { parse, stringify } from 'smol-toml';

const [basePath, overridePath, requestedOutput, mode] = process.argv.slice(2);
if (!basePath || !overridePath || !requestedOutput || process.argv.length > 6
    || (mode !== undefined && mode !== '--fingerprint')) {
  throw new Error('Expected base, override and output public TOML paths, optionally --fingerprint');
}
const [baseSource, overrideSource] = await Promise.all([
  readFile(basePath, 'utf8'), overridePath === '-' ? '' : readFile(overridePath, 'utf8'),
]);
const base = parse(baseSource, { integersAsBigInt: true });
const override = parse(overrideSource, { integersAsBigInt: true });
const contents = stringify(mergeTables(base, override), { numbersAsFloat: true });
const outputPath = mode === '--fingerprint'
  ? join(requestedOutput, createHash('sha256').update(contents).digest('hex').slice(0, 16), 'application.toml')
  : requestedOutput;
const current = await readFile(outputPath, 'utf8').catch(error => {
  if (error.code === 'ENOENT') return null;
  throw error;
});
if (current !== contents) {
  await mkdir(dirname(outputPath), { recursive: true });
  const temporary = `${outputPath}.${process.pid}.tmp`;
  try {
    await writeFile(temporary, contents);
    await rename(temporary, outputPath);
  } finally {
    await rm(temporary, { force: true });
  }
}
if (mode === '--fingerprint') process.stdout.write(`${outputPath}\n`);

function mergeTables(base, override) {
  return Object.fromEntries(Object.entries({ ...base, ...override }).map(([key, value]) => [
    key,
    Object.hasOwn(base, key) && Object.hasOwn(override, key) && isTable(base[key]) && isTable(value)
      ? mergeTables(base[key], value) : value,
  ]));
}

function isTable(value) {
  return value !== null && typeof value === 'object'
    && [Object.prototype, null].includes(Object.getPrototypeOf(value));
}
