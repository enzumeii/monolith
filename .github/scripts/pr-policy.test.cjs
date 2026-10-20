const {test}=require('node:test');
const assert=require('node:assert/strict');
const {parseTitle,parseMessage,bumpVersion}=require('./pr-policy.cjs');
for(const [title,bump] of [['feat: add report','minor'],['chore(deps): update',null],['fix: retry','patch'],['docs: explain',null],['feat!: change contract','major'],['fix(api)!: remove endpoint','major']]) {
  test(title,()=>assert.equal(parseTitle(title).bump,bump));
}
for(const title of ['','feat: ','feat:no space','feat(): empty scope','feat(a(b): nested','fix: one\ntwo','fix: tab\there']) {
  test(`reject ${JSON.stringify(title)}`,()=>assert.throws(()=>parseTitle(title)));
}
for(const [version,bump,want] of [['0.1.0','minor','0.2.0'],['0.1.0','major','1.0.0'],['2.3.4','patch','2.3.5'],['2.3.4','minor','2.4.0'],['2.3.4','major','3.0.0']]) {
  test(`${version} ${bump}`,()=>assert.equal(bumpVersion(version,bump),want));
}
test('reject malformed versions and bump',()=>{
 for(const v of ['v1.2.3','1.2','01.2.3','-1.2.3','1.2.3.4','9007199254740991.0.0']) assert.throws(()=>bumpVersion(v,'major'));
 assert.throws(()=>bumpVersion('1.2.3','other'));
});

for (const [title,bump] of [['FEAT: uppercase','minor'],['FIX(api): fix','patch'],['custom(noun with spaces): describe',null],['ci: pipeline',null]]) {
 test(`specification ${title}`,()=>assert.equal(parseTitle(title).bump,bump));
}
for (const footer of ['BREAKING CHANGE','BREAKING-CHANGE']) {
 test(`breaking footer ${footer}`,()=>assert.equal(parseMessage(`chore: change\n\n${footer}: old format removed`).bump,'major'));
}
test('lowercase footer is not a breaking marker',()=>assert.equal(parseMessage('chore: change\n\nbreaking-change: ordinary footer').bump,null));
test('body requires a blank line',()=>assert.throws(()=>parseMessage('fix: change\nbody without separator')));
test('summary requires colon separator',()=>assert.throws(()=>parseTitle('feat # text')));
for (const footer of ['!: something','BREAKING CHANGE # old','BREAKING CHANGE:old','BREAKING CHANGE: ']) {
 test(`nonstandard breaking footer ${JSON.stringify(footer)}`,()=>assert.equal(parseMessage(`chore: text\n\n${footer}`).bump,null));
}
test('summary breaking marker survives an empty breaking footer',()=>assert.equal(parseMessage('chore!: change\n\nBREAKING CHANGE: ').bump,'major'));
