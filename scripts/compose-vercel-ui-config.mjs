import { readFile, writeFile } from 'node:fs/promises';
import { parse, stringify } from 'smol-toml';

const [basePath, overridePath, outputPath] = process.argv.slice(2);
if (!basePath || !overridePath || !outputPath || process.argv.length !== 5) {
  throw new Error('Expected base, override and output public TOML paths');
}
const [baseSource, overrideSource] = await Promise.all([
  readFile(basePath, 'utf8'), readFile(overridePath, 'utf8'),
]);
const base = parse(baseSource, { integersAsBigInt: true });
const override = parse(overrideSource, { integersAsBigInt: true });
await writeFile(outputPath, stringify(mergeTables(base, override), { numbersAsFloat: true }));

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
