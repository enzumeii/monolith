const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawnSync}=require('node:child_process');
const workflow=fs.readFileSync(path.resolve('.github/workflows/release.yml'),'utf8');
const deploy=workflow.split('\n  deploy:\n')[1]||'';
const release=workflow.split('\n  release:\n')[1].split('\n  deploy:\n')[0];

function step(name) {
 const lines=deploy.split('\n');
 const start=lines.findIndex(line=>line===`      - name: ${name}`);
 assert.notEqual(start,-1,`Missing workflow step: ${name}`);
 const end=lines.findIndex((line,index)=>index>start&&line.startsWith('      - '));
 const block=lines.slice(start,end===-1?undefined:end);
 const run=block.indexOf('        run: |');
 assert.notEqual(run,-1,`Missing Bash body: ${name}`);
 return block.slice(run+1).map(line=>line.slice(10)).join('\n');
}

function fixture(t,overrides={}) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'vercel release '));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 fs.mkdirSync(path.join(root,'bin'));
 fs.mkdirSync(path.join(root,'node_modules/.bin'),{recursive:true});
 fs.writeFileSync(path.join(root,'bin/npm'),`#!/bin/bash
printf 'npm %s\\n' "$*" >> "$TEST_CALLS"
if [ "$1" = ci ]; then exit "\${TEST_INSTALL_EXIT:-0}"; fi
exit "\${TEST_CONFIG_EXIT:-0}"
`,{mode:0o755});
 fs.writeFileSync(path.join(root,'node_modules/.bin/vercel'),`#!/bin/bash
printf '%s\\n' "$PWD" "$@" "$VERCEL_ORG_ID" "$VERCEL_PROJECT_ID" > "$TEST_DEPLOY"
printf 'vercel\\n' >> "$TEST_CALLS"
printf 'Building all services\\n' >&2
if [ "\${TEST_DEPLOY_EXIT:-0}" != 0 ]; then exit "$TEST_DEPLOY_EXIT"; fi
printf 'https://monolith-test.vercel.app\\n'
`,{mode:0o755});
 const summary=path.join(root,'summary'),calls=path.join(root,'calls'),deployment=path.join(root,'deployment');
 const scripts=['Validate Vercel deployment credentials','Install pinned Vercel CLI and validate services','Deploy all services to production'].map(step).join('\n');
 const result=spawnSync('bash',['--noprofile','--norc','-e','-o','pipefail','-c',scripts],{
  cwd:root,encoding:'utf8',env:{...process.env,PATH:path.join(root,'bin')+':'+process.env.PATH,
   VERCEL_TOKEN:'synthetic-token',VERCEL_ORG_ID:'synthetic-team',VERCEL_PROJECT_ID:'synthetic-project',
   GITHUB_STEP_SUMMARY:summary,TEST_CALLS:calls,TEST_DEPLOY:deployment,
   TEST_INSTALL_EXIT:'0',TEST_CONFIG_EXIT:'0',TEST_DEPLOY_EXIT:'0',...overrides}
 });
 const read=file=>fs.existsSync(file)?fs.readFileSync(file,'utf8'):'';
 return {root,result,calls:read(calls),deployment:read(deployment),summary:read(summary)};
}

test('Release button isolates main-only deployment from merged-PR publication',()=>{
 assert.match(workflow,/^  workflow_dispatch:\s*$/m);
 assert.match(workflow,/pull_request_target:\n    types: \[closed\]\n    branches: \[main\]/);
 assert.match(release,/if: github\.event_name == 'pull_request_target' && github\.event\.pull_request\.merged == true/);
 assert.match(release,/ref: main/);
 assert.match(release,/publishReleases\(\{github, \.\.\.context\.repo, commits\}\)/);
 assert.match(deploy,/if: github\.event_name == 'workflow_dispatch' && github\.ref == 'refs\/heads\/main'/);
 assert.match(deploy,/permissions:\n      contents: read\n/);
 assert.doesNotMatch(deploy,/(?:contents|pull-requests|deployments): write|strategy:|matrix:/);
 assert.match(deploy,/ref: \$\{\{ github\.sha \}\}/);
 assert.match(deploy,/persist-credentials: false/);
 assert.match(deploy,/cache-dependency-path: package-lock\.json/);

 for(const name of ['VERCEL_TOKEN','VERCEL_ORG_ID','VERCEL_PROJECT_ID']) {
  assert.ok(deploy.includes(`${name}: \${{ secrets.${name} }}`));
 }
 assert.deepEqual(Object.keys(JSON.parse(fs.readFileSync('vercel.json','utf8')).services).sort(),['api','backoffice','client-ui']);
 assert.equal(JSON.parse(fs.readFileSync('package.json','utf8')).devDependencies.vercel,'65.1.0');
 assert.equal(JSON.parse(fs.readFileSync('package-lock.json','utf8')).packages['node_modules/vercel'].version,'65.1.0');
});

for(const name of ['VERCEL_TOKEN','VERCEL_ORG_ID','VERCEL_PROJECT_ID']) {
 test(`missing ${name} fails before any external command`,t=>{
  const f=fixture(t,{[name]:''});
  assert.notEqual(f.result.status,0);
  assert.match(f.result.stderr,new RegExp(`Missing required GitHub Actions secret: ${name}`));
  assert.equal(f.calls,'');assert.equal(f.deployment,'');assert.equal(f.summary,'');
  assert.doesNotMatch(f.result.stdout+f.result.stderr,/synthetic-token|synthetic-team|synthetic-project/);
 });
}

test('one root production CLI invocation deploys all services and publishes its completed URL',t=>{
 const f=fixture(t);
 assert.equal(f.result.status,0,f.result.stderr);
 assert.equal(f.calls,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\nvercel\n');
 assert.deepEqual(f.deployment.trim().split('\n'),[fs.realpathSync(f.root),'deploy','--prod','--yes','synthetic-team','synthetic-project']);
 assert.match(f.summary,/https:\/\/monolith-test\.vercel\.app/);
 assert.match(f.summary,/backoffice.*client-ui.*api/);
 assert.doesNotMatch(f.result.stdout+f.result.stderr+f.summary,/synthetic-token|synthetic-team|synthetic-project/);
});

for(const [name,variable,exit,calls] of [
 ['dependency installation','TEST_INSTALL_EXIT',29,'npm ci --ignore-scripts --no-audit --no-fund\n'],
 ['service validation','TEST_CONFIG_EXIT',31,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\n'],
 ['deployment/build','TEST_DEPLOY_EXIT',47,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\nvercel\n']
]) {
 test(`${name} failure propagates without a success summary`,t=>{
  const f=fixture(t,{[variable]:String(exit)});
  assert.equal(f.result.status,exit,f.result.stderr);
  assert.equal(f.calls,calls);assert.equal(f.summary,'');
 });
}

test('manual deployment queue cannot replace pending version release runs',()=>{
 assert.doesNotMatch(workflow.split('\njobs:\n')[0],/^concurrency:/m);
 assert.match(release,/concurrency:\n      group: main-releases\n      cancel-in-progress: false/);
 assert.match(deploy,/concurrency:\n      group: vercel-production-deployments\n      cancel-in-progress: false/);
 const group=job=>job.match(/^      group: (.+)$/m)[1];
 // GitHub keeps one pending run per group; another enqueue replaces that pending run.
 const pending=new Map([[group(release),'merged-pr-b']]);
 pending.set(group(deploy),'manual-c');
 pending.set(group(deploy),'manual-d');
 assert.equal(pending.get(group(release)),'merged-pr-b');
 assert.equal(pending.get(group(deploy)),'manual-d');
 assert.equal(pending.size,2);
});
