const fs = require('node:fs');
const modules = {api: 'services/api', backoffice: 'apps/backoffice', 'client-ui': 'apps/client-ui', domain: 'crates/domain', ui: 'crates/ui'};

function validateCoverage(text, crate) {
  if (!modules[crate] || !text.trim().endsWith('end_of_record')) throw new Error('Missing or incomplete LCOV report');
  const result = {files: 0, lines: 0, hit: 0};
  for (const record of text.split('end_of_record').filter(r => r.trim())) {
    const source = /^SF:(.+)$/m.exec(record)?.[1];
    if (!source?.startsWith(`${modules[crate]}/src/`) || source.includes('/tests/') || source.includes('..') || !source.endsWith('.rs')) {
      throw new Error(`Unexpected ${crate} coverage source: ${source}`);
    }
    result.files += 1;
    for (const line of record.split('\n')) {
      const match = /^DA:\d+,(\d+)(?:,.*)?$/.exec(line);
      if (match) { result.lines += 1; if (Number(match[1]) > 0) result.hit += 1; }
    }
  }
  if (!result.files || !result.lines) throw new Error('Empty coverage report');
  return result;
}

if (require.main === module) {
  const [crate, file] = process.argv.slice(2);
  console.log(JSON.stringify(validateCoverage(fs.readFileSync(file, 'utf8'), crate)));
}
module.exports = {validateCoverage};
