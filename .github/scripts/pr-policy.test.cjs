const {test}=require('node:test');
const assert=require('node:assert/strict');
const {parseTitle,bumpVersion}=require('./pr-policy.cjs');
for(const [title,bump] of [['feat: add report','minor'],['chore(deps): update','minor'],['fix: retry','patch'],['docs: explain','patch'],['feat!: change contract','major'],['fix(api)!: remove endpoint','major']]) {
  test(title,()=>assert.equal(parseTitle(title).bump,bump));
}
for(const title of ['','Feat: uppercase','feat: ','feat:no space','unknown: no','feat(): empty scope','feat(a(b): nested','fix: one\ntwo','fix: tab\there']) {
  test(`reject ${JSON.stringify(title)}`,()=>assert.throws(()=>parseTitle(title)));
}
for(const [version,bump,want] of [['0.1.0','minor','0.2.0'],['0.1.0','major','1.0.0'],['2.3.4','patch','2.3.5'],['2.3.4','minor','2.4.0'],['2.3.4','major','3.0.0']]) {
  test(`${version} ${bump}`,()=>assert.equal(bumpVersion(version,bump),want));
}
test('reject malformed versions and bump',()=>{
 for(const v of ['v1.2.3','1.2','01.2.3','-1.2.3','1.2.3.4','9007199254740991.0.0']) assert.throws(()=>bumpVersion(v,'major'));
 assert.throws(()=>bumpVersion('1.2.3','other'));
});
