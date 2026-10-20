const {parser, toConventionalChangelogFormat} = require('@conventional-commits/parser');

function parseTitle(title) {
  if (typeof title !== 'string' || /[\x00-\x1f\x7f]/.test(title)) {
    throw new Error('PR title must be a single Conventional Commit summary');
  }
  return parseMessage(title);
}

function parseMessage(message) {
  const ast = parser(message);
  const summary = ast.children.find(node => node.type === 'summary');
  const separator = summary.children.find(node => node.type === 'separator');
  const scope = summary.children.find(node => node.type === 'scope');
  const commit = toConventionalChangelogFormat(ast);
  if (separator.value !== ':' || message[separator.position.end.offset] !== ' ' || !commit.subject.trim() || (scope && !scope.value.trim())) {
    throw new Error('Conventional Commit requires type(optional scope)!: description');
  }
  const content = ast.children.find(node => node.type === 'body' || node.type === 'footer');
  if (content && !ast.children[1].value.replaceAll('\r', '').startsWith('\n\n')) {
    throw new Error('Body and footers require a blank line after the summary');
  }
  const type = commit.type.toLowerCase();
  const breaking = summary.children.some(node => node.type === 'breaking-change') || ast.children.some(node => {
    if (node.type !== 'footer') return false;
    const token = node.children.find(child => child.type === 'token');
    const separator = node.children.find(child => child.type === 'separator');
    const value = node.children.find(child => child.type === 'value');
    return token.children.some(child => child.value === 'BREAKING CHANGE' || child.value === 'BREAKING-CHANGE')
      && separator.value === ':' && message[separator.position.end.offset] === ' '
      && Boolean(value && message.slice(value.position.start.offset, value.position.end.offset).trim());
  });
  return {type, bump: breaking ? 'major' : type === 'feat' ? 'minor' : type === 'fix' ? 'patch' : null};
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

module.exports = {parseTitle, parseMessage, bumpVersion};
