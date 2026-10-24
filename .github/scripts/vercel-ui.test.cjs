const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const net=require('node:net');
const {spawn,spawnSync}=require('node:child_process');
const {setTimeout:delay}=require('node:timers/promises');

async function server(t, service) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'monolith-vercel-ui-'));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 fs.mkdirSync(path.join(root,'scripts'));
 fs.copyFileSync('scripts/serve-vercel-ui.mjs',path.join(root,'scripts/serve-vercel-ui.mjs'));
 fs.copyFileSync('vercel.json',path.join(root,'vercel.json'));
 const config=JSON.parse(fs.readFileSync('vercel.json'));
 const publicRoot=path.join(root,config.services[service].root,'public');
 const mount=service==='backoffice'?'backoffice':'';
 fs.mkdirSync(path.join(publicRoot,mount,'assets'),{recursive:true});
 fs.writeFileSync(path.join(publicRoot,mount,'index.html'),`<html>${service}</html>`);
 fs.writeFileSync(path.join(publicRoot,mount,'assets','application.toml'),'[api]\nbase_url="/api"\n');
 fs.writeFileSync(path.join(publicRoot,mount,'assets','app.wasm'),'wasm-fixture');
 const socket=net.createServer();
 await new Promise(resolve=>socket.listen(0,'127.0.0.1',resolve));
 const port=socket.address().port;
 await new Promise(resolve=>socket.close(resolve));
 const child=spawn(process.execPath,['scripts/serve-vercel-ui.mjs',service],{cwd:root,env:{...process.env,PORT:String(port)},stdio:'pipe'});
 t.after(()=>child.kill());
 let errors=''; child.stderr.on('data',data=>errors+=data);
 for(let i=0;i<100;i++) {
  try { await fetch(`http://127.0.0.1:${port}/`); break; } catch { if(child.exitCode!==null) assert.fail(errors); await delay(20); }
 }
 return (url,options={})=>fetch(`http://127.0.0.1:${port}${url}`,{redirect:'manual',...options});
}

const checkout='89ed1bf5-9a6c-4a46-847d-e433c579db2d';
test('root portal reloads canonical routes and serves only real assets',async t=>{
 const request=await server(t,'client-ui');
 for(const url of ['/', '/bangkok-2026',`/bangkok-2026/cart/${checkout}`,'/unknown-enabled-state']) {
  const response=await request(url);
  assert.equal(response.status,200,url);
  assert.equal(await response.text(),'<html>client-ui</html>');
 }
 for(const url of ['/assets/application.toml','/assets/app.wasm']) {
  const response=await request(url);
  assert.equal(response.status,200,url);
  assert.doesNotMatch(await response.text(),/<html>/);
 }
 for(const url of ['/assets/missing.wasm','/missing.png','/bangkok/extra','/bangkok/cart/bad-id','/Uppercase','/bad--slug','/-bad','/bad-','/'+ 'a'.repeat(64),'/api','/v1','/healthz','/backoffice','/user/assets/missing.wasm','/api/v1/conventions','/_cron']) {
  assert.equal((await request(url)).status,404,url);
 }
 assert.equal((await request('/',{method:'HEAD'})).status,200);
 assert.equal(await (await request('/',{method:'HEAD'})).text(),'');
 assert.equal((await request('/',{method:'POST'})).status,405);
 assert.equal((await request('/%invalid')).status,400);
});

test('recognized legacy portal routes redirect once with query preserved',async t=>{
 const request=await server(t,'client-ui');
 for(const [url,target] of [['/user','/'],['/user/','/'],['/user/bangkok-2026','/bangkok-2026'],[`/user/bangkok-2026/cart/${checkout}`,`/bangkok-2026/cart/${checkout}`]]) {
  const response=await request(`${url}?page=2&filter=a%20b`);
  assert.equal(response.status,308,url);
  assert.equal(response.headers.get('location'),`${target}?page=2&filter=a%20b`);
 }
 for(const url of ['/user/api','/user/backoffice','/user/bad--slug','/user/bangkok/arbitrary']) assert.equal((await request(url)).status,404,url);
});

test('backoffice asset and SPA behavior survives root portal hosting',async t=>{
 const request=await server(t,'backoffice');
 assert.equal((await request('/backoffice/assets/application.toml')).status,200);
 assert.equal((await request('/backoffice/assets/missing.js')).status,404);
 assert.equal((await request('/backoffice/conventions')).status,200);
 assert.equal((await request('/')).status,404);
});

for(const [app,prefix,basePath] of [['client-ui','root','/'],['backoffice','backoffice','backoffice']]) {
 test(`${app} build publishes config and assets at its intended mount and restores local config`,t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'monolith-ui-build-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  for(const dir of ['scripts',`apps/${app}`,'bin']) fs.mkdirSync(path.join(root,dir),{recursive:true});
  fs.copyFileSync('scripts/build-vercel-ui.sh',path.join(root,'scripts/build-vercel-ui.sh'));
  fs.writeFileSync(path.join(root,`apps/${app}/application.toml`),'local-config');
  fs.writeFileSync(path.join(root,`apps/${app}/application.prod.toml`),'public-config');
  fs.writeFileSync(path.join(root,'bin/dx'),`#!/bin/sh
if [ "$1" = --version ]; then echo 'dioxus 0.7.10 fixture'; exit 0; fi
printf '%s\\n' "$@" > dx-args
mkdir -p "$CARGO_TARGET_DIR/dx/${app}/release/web/public/assets"
printf '<html>built</html>' > "$CARGO_TARGET_DIR/dx/${app}/release/web/public/index.html"
cp application.toml "$CARGO_TARGET_DIR/dx/${app}/release/web/public/assets/application.toml"
exit "\${TEST_BUILD_EXIT:-0}"
`);
  fs.chmodSync(path.join(root,'bin/dx'),0o755);
  const env={...process.env,CARGO_HOME:root,PATH:path.join(root,'bin')+path.delimiter+process.env.PATH};
  const run=extra=>spawnSync('bash',['scripts/build-vercel-ui.sh',app,prefix],{cwd:root,env:{...env,...extra},encoding:'utf8'});
  const built=run({});
  assert.equal(built.status,0,built.stderr);
  const args=fs.readFileSync(path.join(root,`apps/${app}/dx-args`),'utf8').trim().split('\n');
  assert.equal(args[args.indexOf('--base-path')+1],basePath);
  const output=path.join(root,`apps/${app}/vercel/public`,prefix==='root'?'':prefix);
  assert.equal(fs.readFileSync(path.join(output,'assets/application.toml'),'utf8'),'public-config');
  assert.equal(fs.readFileSync(path.join(root,`apps/${app}/application.toml`),'utf8'),'local-config');
  assert.equal(run({TEST_BUILD_EXIT:'17'}).status,17);
  assert.equal(fs.readFileSync(path.join(root,`apps/${app}/application.toml`),'utf8'),'local-config');
 });
}
