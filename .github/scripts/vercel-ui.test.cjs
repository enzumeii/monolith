const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const net=require('node:net');
const {spawn,spawnSync}=require('node:child_process');
const {setTimeout:delay}=require('node:timers/promises');
const {after}=require('node:test');
const compilerRoot=fs.mkdtempSync(path.join(os.tmpdir(),'ui-build-script-'));
after(()=>fs.rmSync(compilerRoot,{recursive:true,force:true}));
const executable=path.join(compilerRoot,'build-script');
const compilation=spawnSync('rustc',['--edition=2024','scripts/ui-config-build.rs','-o',executable],{encoding:'utf8'});
assert.equal(compilation.status,0,compilation.stderr);
const {parse}=require('smol-toml');

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
 for(const url of ['/', '/bangkok-2026', '/bangkok-2026/products/1', '/bangkok-2026/products/900719925474099312345',`/bangkok-2026/cart/${checkout}`,`/bangkok-2026/checkout/return?checkout_id=${checkout}`,`/bangkok-2026/checkout/cancel?checkout_id=${checkout}&token=synthetic`,'/unknown-enabled-state']) {
  const response=await request(url);
  assert.equal(response.status,200,url);
  assert.equal(await response.text(),'<html>client-ui</html>');
 }
 for(const url of ['/assets/application.toml','/assets/app.wasm']) {
  const response=await request(url);
  assert.equal(response.status,200,url);
  assert.doesNotMatch(await response.text(),/<html>/);
 }
 for(const url of ['/assets/missing.wasm','/missing.png','/bangkok/extra','/bangkok/checkout/invalid','/bangkok/checkout/return/extra','/bangkok/products/bad','/bangkok/products/0','/bangkok/products/01','/bangkok/products/1/extra','/bangkok/cart/bad-id','/Uppercase','/bad--slug','/-bad','/bad-','/'+ 'a'.repeat(64),'/api','/v1','/healthz','/backoffice','/user/assets/missing.wasm','/api/v1/conventions','/_cron']) {
  assert.equal((await request(url)).status,404,url);
 }
 assert.equal((await request('/',{method:'HEAD'})).status,200);
 assert.equal(await (await request('/',{method:'HEAD'})).text(),'');
 assert.equal((await request('/',{method:'POST'})).status,405);
 assert.equal((await request('/%invalid')).status,400);
});

test('recognized legacy portal routes redirect once with query preserved',async t=>{
 const request=await server(t,'client-ui');
 for(const [url,target] of [['/user','/'],['/user/','/'],['/user/bangkok-2026','/bangkok-2026'],['/user/bangkok-2026/products/1','/bangkok-2026/products/1'],[`/user/bangkok-2026/cart/${checkout}`,`/bangkok-2026/cart/${checkout}`],['/user/bangkok-2026/checkout/return','/bangkok-2026/checkout/return'],['/user/bangkok-2026/checkout/cancel','/bangkok-2026/checkout/cancel']]) {
  const response=await request(`${url}?page=2&filter=a%20b`);
  assert.equal(response.status,308,url);
  assert.equal(response.headers.get('location'),`${target}?page=2&filter=a%20b`);
 }
 for(const url of ['/user/api','/user/backoffice','/user/bad--slug','/user/bangkok/arbitrary','/user/bangkok/products/0','/user/bangkok/products/bad']) assert.equal((await request(url)).status,404,url);
});

test('backoffice asset and SPA behavior survives root portal hosting',async t=>{
 const request=await server(t,'backoffice');
 assert.equal((await request('/backoffice/assets/application.toml')).status,200);
 assert.equal((await request('/backoffice/assets/missing.js')).status,404);
 assert.equal((await request('/backoffice/conventions')).status,200);
 assert.equal((await request('/')).status,404);
});

for(const [app,prefix,basePath] of [['client-ui','root','/'],['backoffice','backoffice','backoffice']]) {
 test(`${app} build publishes composed production config without rewriting source`,t=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'monolith-ui-build-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  for(const dir of ['scripts',`apps/${app}`,'bin']) fs.mkdirSync(path.join(root,dir),{recursive:true});
  fs.copyFileSync('scripts/build-vercel-ui.sh',path.join(root,'scripts/build-vercel-ui.sh'));
  fs.copyFileSync('scripts/compose-vercel-ui-config.mjs',path.join(root,'scripts/compose-vercel-ui-config.mjs'));
  fs.symlinkSync(path.resolve('node_modules'),path.join(root,'node_modules'),'dir');
  const local=fs.readFileSync(`apps/${app}/application.toml`,'utf8')+'\n[build_fixture]\nretained = true\n';
  fs.writeFileSync(path.join(root,`apps/${app}/application.toml`),local);

  fs.writeFileSync(path.join(root,'bin/dx'),`#!/bin/sh
if [ "$1" = --version ]; then echo 'dioxus 0.7.10 fixture'; exit 0; fi
printf '%s\\n' "$@" > dx-args
mkdir -p "$CARGO_TARGET_DIR/dx/${app}/release/web/public/assets"
printf '<html>built</html>' > "$CARGO_TARGET_DIR/dx/${app}/release/web/public/index.html"
CARGO_MANIFEST_DIR="$PWD" PROFILE=release "${executable}" > build-script-output || exit 1
asset=$(sed -n 's/^cargo:rustc-env=PUBLIC_CONFIG_ASSET=//p' build-script-output)
cp ".$asset" "$CARGO_TARGET_DIR/dx/${app}/release/web/public/assets/application.toml"
exit "\${TEST_BUILD_EXIT:-0}"
`);
  fs.chmodSync(path.join(root,'bin/dx'),0o755);
  const env={...process.env,CARGO_HOME:root,PATH:path.join(root,'bin')+path.delimiter+process.env.PATH};
  const run=(extra,selected)=>spawnSync('bash',['scripts/build-vercel-ui.sh',app,prefix,...(selected?[selected]:[])],{cwd:root,env:{...env,...extra},encoding:'utf8'});
  const built=run({});
  assert.equal(built.status,0,built.stderr);
  const args=fs.readFileSync(path.join(root,`apps/${app}/dx-args`),'utf8').trim().split('\n');
  assert.equal(args[args.indexOf('--base-path')+1],basePath);
  const output=path.join(root,`apps/${app}/vercel/public`,prefix==='root'?'':prefix);
  const published=parse(fs.readFileSync(path.join(output,'assets/application.toml'),'utf8'));
  assert.equal(published.api.base_url,'/api/v1');
  assert.equal(published.build_fixture?.retained,true,'unspecified base settings survive for both apps');
  if(app==='backoffice') assert.equal(published.conventions.catalogue_location_id,'65761018027');
  const source=path.join(root,`apps/${app}/application.toml`);
  assert.equal(fs.readFileSync(source,'utf8'),local);
  assert.equal(run({TEST_BUILD_EXIT:'17'}).status,17);
  assert.equal(fs.readFileSync(source,'utf8'),local);
  assert.equal(run({},'application.toml').status,0,'base file remains a valid selected config');
  assert.deepEqual(parse(fs.readFileSync(path.join(output,'assets/application.toml'),'utf8')),parse(local));
  assert.equal(fs.readFileSync(source,'utf8'),local);
  fs.rmSync(path.join(root,`apps/${app}/vercel`),{recursive:true});
  fs.writeFileSync(path.join(root,`apps/${app}/application.prod.toml`),'[invalid');
  assert.notEqual(run({}).status,0,'malformed override fails');
  assert.equal(fs.readFileSync(source,'utf8'),local);
  assert.equal(fs.existsSync(path.join(output,'assets/application.toml')),false,'failed composition publishes no asset');
  fs.unlinkSync(path.join(root,`apps/${app}/application.prod.toml`));
  assert.notEqual(run({},'missing.toml').status,0,'missing explicit override fails');
  assert.equal(fs.readFileSync(source,'utf8'),local);
  assert.equal(fs.existsSync(path.join(output,'assets/application.toml')),false);
 });
}
