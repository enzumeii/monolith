const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawnSync}=require('node:child_process');
const {parse}=require('smol-toml');

function fixture(t,base,override) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'monolith-ui-config-'));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 const source=path.join(root,'application.toml');
 const overlay=path.join(root,'application.prod.toml');
 const output=path.join(root,'published.toml');
 fs.writeFileSync(source,base);
 fs.writeFileSync(overlay,override);
 return {source,overlay,output,run:(selected=overlay,target=output)=>spawnSync(process.execPath,['scripts/compose-vercel-ui-config.mjs',source,selected,target],{encoding:'utf8'})};
}

test('public TOML overrides merge nested tables and replace arrays, scalars and table shapes',t=>{
 const f=fixture(t,`title="base"
[api]
base_url="http://localhost:3000/v1"
timeout=10
[api.headers]
accept="application/json"
[display]
columns=[1,2,3]
mode={enabled=true}
old="scalar"
[[regions]]
name="base"
`, `[api]
base_url="/api/v1"
[api.headers]
origin="same-host"
[display]
columns=[4]
mode="compact"
old={enabled=true}
[[regions]]
name="production"
`);
 assert.equal(f.run().status,0);
 assert.deepEqual(parse(fs.readFileSync(f.output,'utf8')),{
  title:'base',api:{base_url:'/api/v1',timeout:10,headers:{accept:'application/json',origin:'same-host'}},
  display:{columns:[4],mode:'compact',old:{enabled:true}},regions:[{name:'production'}],
 });
});

test('composition preserves TOML dates, integer precision, floats and quoted keys',t=>{
 const base=`date=1979-05-27
time=07:32:00
instant=1979-05-27T07:32:00-08:00
local=1979-05-27T07:32:00
integer=9223372036854775807
float=1.0
boolean=true
"a.b"="literal"
["__proto__"]
value="base"
`;
 const f=fixture(t,base,'date=2026-10-24\n["__proto__"]\nother="override"\n');
 assert.equal(f.run().status,0);
 const result=parse(fs.readFileSync(f.output,'utf8'),{integersAsBigInt:true});
 const expected=parse(base,{integersAsBigInt:true});
 expected.date=parse('date=2026-10-24').date;
 expected.__proto__.other='override';
 assert.deepEqual(result,expected);
 assert.match(fs.readFileSync(f.output,'utf8'),/float = 1\.0/);
 assert.equal({}.value,undefined);
});

for(const [name,contents] of [['missing',null],['malformed','[broken']]) {
 for(const input of ['source','overlay']) {
  test(`${name} ${input} fails without modifying existing output`,t=>{
   const f=fixture(t,'[api]\nbase_url="local"\n','[api]\nbase_url="/api/v1"\n');
   fs.writeFileSync(f.output,'existing output');
   if(contents===null) fs.unlinkSync(f[input]); else fs.writeFileSync(f[input],contents);
   assert.notEqual(f.run().status,0);
   assert.equal(fs.readFileSync(f.output,'utf8'),'existing output');
  });
 }
}

test('selected source and output can be the same application.toml',t=>{
 const f=fixture(t,'[api]\nbase_url="local"\n','[api]\nbase_url="/api/v1"\n');
 assert.equal(f.run(f.source,f.source).status,0);
 assert.deepEqual(parse(fs.readFileSync(f.source,'utf8')),{api:{base_url:'local'}});
});
