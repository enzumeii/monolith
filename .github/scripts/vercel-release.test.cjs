const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const {spawnSync}=require('node:child_process');
const workflow=fs.readFileSync(path.resolve('.github/workflows/release.yml'),'utf8');
const deploy=workflow.split('\n  deploy:\n')[1]||'';
const release=workflow.split('\n  release:\n')[1].split('\n  deploy:\n')[0];

function block(name) {
 const lines=deploy.split('\n');
 const start=lines.findIndex(line=>line===`      - name: ${name}`);
 assert.notEqual(start,-1,`Missing workflow step: ${name}`);
 const end=lines.findIndex((line,index)=>index>start&&line.startsWith('      - '));
 const block=lines.slice(start,end===-1?undefined:end);
 return block;
}

function step(name) {
 const lines=block(name);
 const run=lines.findIndex(line=>line.startsWith('        run:'));
 assert.notEqual(run,-1,`Missing Bash body: ${name}`);
 if(lines[run]!=='        run: |') return lines[run].slice('        run: '.length);
 return lines.slice(run+1).map(line=>line.slice(10)).join('\n');
}

function fixture(t,overrides={}) {
 const root=fs.mkdtempSync(path.join(os.tmpdir(),'vercel release '));
 t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
 fs.mkdirSync(path.join(root,'bin'));
 fs.mkdirSync(path.join(root,'node_modules/.bin'),{recursive:true});
 fs.writeFileSync(path.join(root,'bin/npm'),`#!/bin/bash
test -z "\${DATABASE_URL:-}\${SUPABASE_MIGRATION_DATABASE_URL:-}" || exit 91
printf 'npm %s\\n' "$*" >> "$TEST_CALLS"
if [ "$1" = ci ]; then exit "\${TEST_INSTALL_EXIT:-0}"; fi
exit "\${TEST_CONFIG_EXIT:-0}"
`,{mode:0o755});
 fs.writeFileSync(path.join(root,'bin/cargo'),`#!/bin/bash
test -z "\${DATABASE_URL:-}\${SUPABASE_MIGRATION_DATABASE_URL:-}" || exit 91
printf 'cargo %s\\n' "$*" >> "$TEST_CALLS"
exit "\${TEST_SQLX_INSTALL_EXIT:-0}"
`,{mode:0o755});
 fs.writeFileSync(path.join(root,'bin/sqlx'),`#!/bin/bash
test -z "\${SUPABASE_MIGRATION_DATABASE_URL:-}" || exit 91
printf 'sqlx %s\\n' "$*" >> "$TEST_CALLS"
printf '%s\\n' "$DATABASE_URL" > "$TEST_MIGRATION"
exit "\${TEST_MIGRATION_EXIT:-0}"
`,{mode:0o755});
 fs.writeFileSync(path.join(root,'node_modules/.bin/vercel'),`#!/bin/bash
test -z "\${DATABASE_URL:-}\${SUPABASE_MIGRATION_DATABASE_URL:-}" || exit 91
printf '%s\\n' "$PWD" "$@" "$VERCEL_ORG_ID" "$VERCEL_PROJECT_ID" > "$TEST_DEPLOY"
printf 'vercel\\n' >> "$TEST_CALLS"
printf 'Building all services\\n' >&2
if [ "\${TEST_DEPLOY_EXIT:-0}" != 0 ]; then exit "$TEST_DEPLOY_EXIT"; fi
printf 'https://monolith-test.vercel.app\\n'
`,{mode:0o755});
 const summary=path.join(root,'summary'),calls=path.join(root,'calls'),deployment=path.join(root,'deployment');
 const secret='postgresql://postgres:synthetic-password@db.example.test:5432/postgres?sslmode=require';
 const scripts=[...deploy.matchAll(/^      - name: (.+)$/gm)].map(match=>match[1])
  .filter(name=>block(name).some(line=>line.startsWith('        run:')));
 const migration=path.join(root,'migration');
 const secrets={VERCEL_TOKEN:'synthetic-token',VERCEL_ORG_ID:'synthetic-team',VERCEL_PROJECT_ID:'synthetic-project',SUPABASE_MIGRATION_DATABASE_URL:secret,...overrides};
 const results=[];
 for(const name of scripts) {
  const scoped={};
  for(const line of block(name)) {
   const binding=line.match(/^          (\w+): \$\{\{ secrets\.(\w+) \}\}$/);
   if(binding) scoped[binding[1]]=secrets[binding[2]];
  }
  const result=spawnSync('bash',['--noprofile','--norc','-e','-o','pipefail','-c',step(name)],{
  cwd:root,encoding:'utf8',env:{...process.env,PATH:path.join(root,'bin')+':'+process.env.PATH,
   VERCEL_TOKEN:'synthetic-token',VERCEL_ORG_ID:'synthetic-team',VERCEL_PROJECT_ID:'synthetic-project',
   GITHUB_STEP_SUMMARY:summary,TEST_CALLS:calls,TEST_DEPLOY:deployment,
   TEST_MIGRATION:migration,TEST_SQLX_INSTALL_EXIT:'0',TEST_MIGRATION_EXIT:'0',
   TEST_INSTALL_EXIT:'0',TEST_CONFIG_EXIT:'0',TEST_DEPLOY_EXIT:'0',...overrides,
   DATABASE_URL:'',SUPABASE_MIGRATION_DATABASE_URL:'',...scoped}
 });
  results.push(result);
  if(result.status!==0) break;
 }
 const result={status:results.at(-1).status,stdout:results.map(r=>r.stdout).join(''),stderr:results.map(r=>r.stderr).join('')};
 const read=file=>fs.existsSync(file)?fs.readFileSync(file,'utf8'):'';
 return {root,result,calls:read(calls),deployment:read(deployment),summary:read(summary),migration:read(migration),secret};
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

 for(const name of ['VERCEL_TOKEN','VERCEL_ORG_ID','VERCEL_PROJECT_ID','SUPABASE_MIGRATION_DATABASE_URL']) {
  assert.ok(deploy.includes(`${name}: \${{ secrets.${name} }}`));
 }
 assert.deepEqual(Object.keys(JSON.parse(fs.readFileSync('vercel.json','utf8')).services).sort(),['api','backoffice','client-ui']);
 assert.equal(JSON.parse(fs.readFileSync('package.json','utf8')).devDependencies.vercel,'65.1.0');
 assert.equal(JSON.parse(fs.readFileSync('package-lock.json','utf8')).packages['node_modules/vercel'].version,'65.1.0');
});

for(const name of ['VERCEL_TOKEN','VERCEL_ORG_ID','VERCEL_PROJECT_ID','SUPABASE_MIGRATION_DATABASE_URL']) {
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
 assert.equal(f.calls,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\ncargo install --locked sqlx-cli --version 0.9.0 --no-default-features --features postgres,rustls\nsqlx --no-dotenv migrate run --source services/api/migrations\nvercel\n');
 assert.equal(f.migration.trim(),f.secret);
 assert.ok(!(f.result.stdout+f.result.stderr+f.summary+f.calls+f.deployment).includes(f.secret));
 assert.deepEqual(f.deployment.trim().split('\n'),[fs.realpathSync(f.root),'deploy','--prod','--yes','synthetic-team','synthetic-project']);
 assert.match(f.summary,/https:\/\/monolith-test\.vercel\.app/);
 assert.match(f.summary,/backoffice.*client-ui.*api/);
 assert.doesNotMatch(f.result.stdout+f.result.stderr+f.summary,/synthetic-token|synthetic-team|synthetic-project/);
});

for(const [name,variable,exit,calls] of [
 ['dependency installation','TEST_INSTALL_EXIT',29,'npm ci --ignore-scripts --no-audit --no-fund\n'],
 ['service validation','TEST_CONFIG_EXIT',31,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\n'],
 ['migration CLI installation','TEST_SQLX_INSTALL_EXIT',37,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\ncargo install --locked sqlx-cli --version 0.9.0 --no-default-features --features postgres,rustls\n'],
 ['database migration','TEST_MIGRATION_EXIT',43,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\ncargo install --locked sqlx-cli --version 0.9.0 --no-default-features --features postgres,rustls\nsqlx --no-dotenv migrate run --source services/api/migrations\n'],
 ['deployment/build','TEST_DEPLOY_EXIT',47,'npm ci --ignore-scripts --no-audit --no-fund\nnpm run check:vercel\ncargo install --locked sqlx-cli --version 0.9.0 --no-default-features --features postgres,rustls\nsqlx --no-dotenv migrate run --source services/api/migrations\nvercel\n']
]) {
 test(`${name} failure propagates without a success summary`,t=>{
  const f=fixture(t,{[variable]:String(exit)});
  assert.equal(f.result.status,exit,f.result.stderr);
  assert.equal(f.calls,calls);assert.equal(f.summary,'');
  if(variable!=='TEST_DEPLOY_EXIT') assert.equal(f.deployment,'');
  assert.ok(!(f.result.stdout+f.result.stderr+f.summary+f.calls).includes(f.secret));
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

test('migration secret is scoped to preflight and SQLx only',()=>{
 const jobEnvironment=deploy.split('    steps:')[0];
 assert.doesNotMatch(jobEnvironment,/SUPABASE|DATABASE_URL/);
 assert.match(block('Migrate Supabase before deployment').join('\n'),/DATABASE_URL: \$\{\{ secrets\.SUPABASE_MIGRATION_DATABASE_URL \}\}/);
 assert.doesNotMatch(step('Migrate Supabase before deployment'),/--database-url|\$DATABASE_URL|\$SUPABASE/);
 assert.doesNotMatch(block('Deploy all services to production').join('\n'),/DATABASE_URL|SUPABASE/);
});
