const types = new Set(['feat', 'fix', 'chore', 'docs', 'style', 'refactor', 'perf', 'test', 'build', 'ci', 'revert']);

function parseTitle(title) {
  const match = typeof title === 'string' && !/[\x00-\x1f\x7f]/.test(title)
    && /^([a-z]+)(?:\(([a-zA-Z0-9][a-zA-Z0-9_./-]*)\))?(!)?: (\S.*)$/.exec(title);
  if (!match || !types.has(match[1]) || title !== title.trim()) {
    throw new Error('PR title must be type(optional-scope)!: description using a supported Conventional Commit type');
  }
  return {type: match[1], bump: match[3] ? 'major' : ['feat', 'chore'].includes(match[1]) ? 'minor' : 'patch'};
}

function bumpVersion(version, bump) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) throw new Error('Invalid version');
  const parts = version.split('.').map(Number);
  const index = ['major', 'minor', 'patch'].indexOf(bump);
  if (index < 0 || parts.some(v => !Number.isSafeInteger(v))) throw new Error('Invalid version or bump');
  parts[index] += 1;
  if (!Number.isSafeInteger(parts[index])) throw new Error('Version overflow');
  for (let i = index + 1; i < 3; i += 1) parts[i] = 0;
  return parts.join('.');
}

module.exports = {parseTitle, bumpVersion};
