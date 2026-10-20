const {test}=require('node:test');
const assert=require('node:assert/strict');
const {validateCoverage}=require('./coverage-report.cjs');
test('counts scoped production hits',()=>assert.deepEqual(validateCoverage('SF:services/api/src/main.rs\nDA:1,2\nDA:2,0\nend_of_record\n','api'),{files:1,lines:2,hit:1}));
for(const text of ['', 'SF:/tmp/services/api/src/main.rs\nend_of_record\n','SF:crates/domain/src/lib.rs\nend_of_record\n','SF:services/api/src/tests/auth.rs\nend_of_record\n']) test(`reject incomplete or foreign report ${text}`,()=>assert.throws(()=>validateCoverage(text,'api')));
test('permits zero hit source in untested placeholder crate',()=>assert.equal(validateCoverage('SF:apps/client-ui/src/main.rs\nDA:1,0\nend_of_record\n','client-ui').hit,0));
