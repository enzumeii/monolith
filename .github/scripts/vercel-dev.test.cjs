const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawn,spawnSync}=require('node:child_process');
const {setTimeout:delay}=require('node:timers/promises');

function fixture(t, {dotenv='', inherited={}, execute=true}={}) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'monolith-vercel-dev-'));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 for(const dir of ['scripts','services/api','node_modules/vercel/dist']) fs.mkdirSync(path.join(root,dir),{recursive:true});
 fs.copyFileSync('scripts/dev-vercel.mjs',path.join(root,'scripts/dev-vercel.mjs'));
 fs.copyFileSync('vercel.json',path.join(root,'vercel.json'));
 fs.writeFileSync(path.join(root,'services/api/.env'),dotenv);
 fs.writeFileSync(path.join(root,'scripts/build-vercel-ui.sh'),'exit 0\n');
 fs.writeFileSync(path.join(root,'node_modules/vercel/dist/index.js'),`
const fs=require('node:fs');
const configPath=process.argv[process.argv.indexOf('--local-config')+1];
const config=JSON.parse(fs.readFileSync(configPath));
fs.writeFileSync('observed.json',JSON.stringify({config,configPath,runEnv:process.env.RUN_ENV,vercelEnv:process.env.VERCEL_ENV,url:process.env.VERCEL_URL,db:process.env.DATABASE_URL,jwt:process.env.JWT_SECRET,cron:process.env.CRON_SECRET}));
process.exit(7);
`);
 const env={...process.env};
 for(const key of ['RUN_ENV','VERCEL_ENV','VERCEL_URL','DATABASE_URL','JWT_SECRET','CRON_SECRET']) delete env[key];
 if(!execute) return {root,env:{...env,...inherited}};
 const result=spawnSync(process.execPath,['scripts/dev-vercel.mjs'],{cwd:root,env:{...env,...inherited},encoding:'utf8'});
 return {root,result,observed:fs.existsSync(path.join(root,'observed.json'))?JSON.parse(fs.readFileSync(path.join(root,'observed.json'))):null};
}

test('local dev preflight reports missing setting names without secret values',t=>{
 const {result,observed}=fixture(t,{dotenv:'JWT_SECRET=do-not-log-this\n'});
 assert.equal(result.status,1);
 assert.match(result.stderr,/DATABASE_URL/);
 assert.match(result.stderr,/CRON_SECRET/);
 assert.doesNotMatch(result.stderr,/do-not-log-this/);
 assert.equal(observed,null);
});

test('local dev loads API dotenv, preserves inherited credentials, uses native API and removes temporary config',t=>{
 const {root,result,observed}=fixture(t,{dotenv:'RUN_ENV=prod\nDATABASE_URL=postgres://synthetic:synthetic@localhost/local\nJWT_SECRET=file-key\nCRON_SECRET=file-cron\n',inherited:{JWT_SECRET:'inherited-key'}});
 assert.equal(result.status,7);
 assert.equal(observed.jwt,'inherited-key');
 assert.equal(observed.cron,'file-cron');
 assert.equal(observed.db,'postgres://synthetic:synthetic@localhost/local');
 assert.equal(observed.runEnv,'local');
 assert.equal(observed.vercelEnv,'development');
 assert.equal(observed.url,'localhost:18027');
 assert.deepEqual(observed.config.services.api,{root:'.',devCommand:'cd services/api && cargo run --locked -p api -- server'});
 assert.deepEqual(observed.config.rewrites,JSON.parse(fs.readFileSync('vercel.json')).rewrites);
 assert.equal(fs.existsSync(path.join(root,observed.configPath)),false);
 assert.equal(fs.readdirSync(root).filter(name=>name.startsWith('.vercel-dev-')).length,0);
 assert.doesNotMatch(result.stdout+result.stderr,/inherited-key|file-key|file-cron/);
});

for(const signal of ['SIGINT','SIGTERM']) {
 test(`local dev ${signal} stops descendant processes and removes temporary config`,{timeout:10000},async t=>{
  const {root,env}=fixture(t,{execute:false,dotenv:'DATABASE_URL=synthetic\nJWT_SECRET=synthetic\nCRON_SECRET=synthetic\n'});
  fs.writeFileSync(path.join(root,'node_modules/vercel/dist/index.js'),`
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const descendant=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});
fs.writeFileSync('descendant.pid',String(descendant.pid));
setInterval(()=>{},1000);
`);
  const child=spawn(process.execPath,['scripts/dev-vercel.mjs'],{cwd:root,env,stdio:'ignore'});
  let descendant;
  t.after(()=>{try{child.kill('SIGKILL');}catch{} if(descendant)try{process.kill(descendant,'SIGKILL');}catch{}});
  const done=new Promise(resolve=>child.once('exit',(code)=>resolve(code)));
  for(let i=0;i<100&&!fs.existsSync(path.join(root,'descendant.pid'));i++) await delay(20);
  assert.ok(fs.existsSync(path.join(root,'descendant.pid')),'CLI started');
  descendant=Number(fs.readFileSync(path.join(root,'descendant.pid'),'utf8'));
  child.kill(signal);
  assert.equal(await done,signal==='SIGINT'?130:143);
  let alive=true;
  for(let i=0;i<100;i++) {
   try{process.kill(descendant,0);}catch(error){if(error.code==='ESRCH'){alive=false;break;}throw error;}
   await delay(20);
  }
  assert.equal(alive,false,'descendant stopped');
  assert.equal(fs.readdirSync(root).filter(name=>name.startsWith('.vercel-dev-')).length,0);
 });
}
