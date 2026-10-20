const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawnSync}=require('node:child_process');
const script=path.resolve('scripts/test-coverage.sh');
function fixture(t) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'coverage scope '));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 fs.mkdirSync(path.join(root,'scripts'));fs.mkdirSync(path.join(root,'bin'));
 fs.copyFileSync(script,path.join(root,'scripts/test-coverage.sh'));
 fs.writeFileSync(path.join(root,'bin/just'),'#!/bin/bash\nexit "${TEST_EXIT:-0}"\n',{mode:0o755});
 fs.writeFileSync(path.join(root,'bin/cargo'),`#!/bin/bash
case "$2" in
show-env) printf 'export INSTRUMENTED=1\\n';;
report) while [ "$1" != "--output-path" ]; do shift; done; shift; printf 'SF:services/api/src/main.rs\\nDA:1,1\\nend_of_record\\n' > "$1";;
esac
`,{mode:0o755});
 return {root,run:(...args)=>spawnSync('bash',[path.join(root,'scripts/test-coverage.sh'),...args],{encoding:'utf8',env:{...process.env,PATH:path.join(root,'bin')+':'+process.env.PATH,TEST_EXIT:'0'}})};
}
test('ordinary tests succeed without coverage output',t=>{const f=fixture(t);const r=f.run('api');assert.equal(r.status,0,r.stderr);assert.equal(fs.existsSync(path.join(f.root,'target')),false);});
test('coverage writes only its scope even with spaces in path',t=>{const f=fixture(t);const r=f.run('api','--coverage');assert.equal(r.status,0,r.stderr);assert.match(fs.readFileSync(path.join(f.root,'target/coverage/api/lcov.info'),'utf8'),/SF:services\/api/);});
test('unknown scope or option fails',t=>{const f=fixture(t);assert.notEqual(f.run('unknown').status,0);assert.notEqual(f.run('api','--typo').status,0);});
test('test failure removes stale report and preserves other scopes',t=>{
 const f=fixture(t);for(const crate of ['api','domain']){fs.mkdirSync(path.join(f.root,'target/coverage',crate),{recursive:true});fs.writeFileSync(path.join(f.root,'target/coverage',crate,'lcov.info'),'stale');}
 fs.writeFileSync(path.join(f.root,'bin/just'),'#!/bin/bash\nexit 42\n',{mode:0o755});
 assert.equal(f.run('api','--coverage').status,42);
 assert.equal(fs.existsSync(path.join(f.root,'target/coverage/api/lcov.info')),false);
 assert.equal(fs.readFileSync(path.join(f.root,'target/coverage/domain/lcov.info'),'utf8'),'stale');
});
