const {test}=require('node:test');
const assert=require('node:assert/strict');
const {publishReleases}=require('./release.cjs');
function fixture() {
 const tags=new Map(), releases=new Map(), notes=[];
 const pulls={a:{number:26,title:'fix: retry',merged_at:'2026-10-20',merge_commit_sha:'a',base:{ref:'main'},html_url:'https://example/26'},b:{number:27,title:'feat: add',merged_at:'2026-10-21',merge_commit_sha:'b',base:{ref:'main'},html_url:'https://example/27'}};
 const missing=()=>{throw Object.assign(new Error('Not found'),{status:404});};
 const messages={}, associations={};
 for (const pr of Object.values(pulls)) { pr.merged=true; pr.commits=1; }
 const parents={a:['baseline'],b:['a']};
 const github={rest:{repos:{
 getCommit:async({ref})=>({data:{sha:ref,parents:(parents[ref]||[]).map(sha=>({sha})),commit:{message:messages[ref]||pulls[ref]?.title||'fix: part'}}}),
 listReleases:async()=>({data:[...releases.values()]}),
 listPullRequestsAssociatedWithCommit:async({commit_sha})=>({data:associations[commit_sha]||(pulls[commit_sha]?[pulls[commit_sha]]:[])}),
 getReleaseByTag:async({tag})=>({data:releases.get(tag)||missing()}),
 generateReleaseNotes:async(p)=>{notes.push(p);return {data:{body:'Generated notes'}};},
 createRelease:async(p)=>{releases.set(p.tag_name,p);return {data:p};}
 },pulls:{get:async({pull_number})=>({data:Object.values(pulls).find(pr=>pr.number===pull_number)||missing()})},git:{
 getRef:async({ref})=>({data:{object:{sha:tags.get(ref)||missing(),type:'commit'}}}),
 createRef:async({ref,sha})=>{tags.set(ref.replace('refs/',''),sha);}
 }},paginate:async(fn,p)=>(await fn(p)).data};
 return {github,tags,releases,pulls,messages,associations,parents,notes};
}
const run=(f,commits=['a'],dryRun=false)=>publishReleases({github:f.github,owner:'o',repo:'r',commits,dryRun});
test('initial patch then chore minor, exact target and bounded notes',async()=>{
 const f=fixture();await run(f,['a','b']);
 assert.deepEqual([...f.tags],[['tags/v0.1.1','a'],['tags/v0.2.0','b']]);
 assert.equal(f.releases.get('v0.1.1').target_commitish,'a');
 assert.match(f.releases.get('v0.1.1').body,/https:\/\/example\/26/);
 assert.equal(f.releases.get('v0.2.0').body,'Generated notes');
});
test('replay does not duplicate versions',async()=>{const f=fixture();await run(f);await run(f);assert.equal(f.releases.size,1);});
test('tag-only retry completes release',async()=>{const f=fixture();f.tags.set('tags/v0.1.1','a');await run(f);assert.equal(f.releases.size,1);});
test('dry-run performs no writes',async()=>{const f=fixture();const r=await run(f,['a','b'],true);assert.equal(r.length,2);assert.equal(f.tags.size,0);assert.equal(f.releases.size,0);});
test('breaking overrides type at 0.x',async()=>{const f=fixture();f.pulls.a.title='fix!: remove';await run(f);assert.equal(f.tags.get('tags/v1.0.0'),'a');});
for(const invalid of ['unmerged','other-base','other-sha','no-pr','bad-title','ambiguous']) {
 test(`reject ${invalid}`,async()=>{
  const f=fixture();if(invalid==='unmerged')f.pulls.a.merged_at=null;
  if(invalid==='other-base')f.pulls.a.base.ref='dev';
  if(invalid==='other-sha')f.pulls.a.merge_commit_sha='other';
  if(invalid==='no-pr')delete f.pulls.a;
  if(invalid==='bad-title')f.pulls.a.title='invalid';
  if(invalid==='ambiguous')f.github.paginate=async()=>[f.pulls.a,{...f.pulls.a,number:999}];
  await assert.rejects(run(f));assert.equal(f.tags.size,0);
 });
}
test('tag collision never moves a tag',async()=>{const f=fixture();f.tags.set('tags/v0.1.1','other');await assert.rejects(run(f),/conflict/i);assert.equal(f.tags.get('tags/v0.1.1'),'other');});
test('API authorization errors propagate',async()=>{const f=fixture();f.github.rest.git.getRef=async()=>{throw Object.assign(new Error('Forbidden'),{status:403});};await assert.rejects(run(f),/Forbidden/);});
test('release-create failure resumes on retry',async()=>{const f=fixture();const create=f.github.rest.repos.createRelease;f.github.rest.repos.createRelease=async()=>{throw Error('unavailable');};await assert.rejects(run(f));f.github.rest.repos.createRelease=create;await run(f);assert.equal(f.releases.size,1);});
test('replay resumes after the latest verified release without rereading old PRs',async()=>{
 const f=fixture();await run(f,['a']);
 f.github.rest.repos.listPullRequestsAssociatedWithCommit=async({commit_sha})=>{if(commit_sha==='a')throw Error('old PR should not be read');return {data:[f.pulls.b]};};
 await run(f,['a','b']);assert.equal(f.releases.get('v0.2.0').target_commitish,'b');
});

test('chore has no implicit version effect',async()=>{const f=fixture();f.pulls.a.title='chore: update';assert.equal((await run(f)).length,0);assert.equal(f.tags.size,0);});
test('breaking footer in merge message overrides a chore title',async()=>{const f=fixture();f.pulls.a.title='chore: update';f.messages.a='chore: update\n\nBREAKING CHANGE: format removed';await run(f);assert.equal(f.tags.get('tags/v1.0.0'),'a');});

for (const method of ['merge','squash']) {
 test(`${method} stack reconciles three layers once in landed order`,async()=>{
  const f=fixture();
  f.pulls.b.base.ref='feature/lower';
  f.pulls.b.stack={base:{ref:'main',sha:null},id:2458232,number:32,position:2,size:3};
  f.pulls.c={...f.pulls.b,number:28,title:'fix!: remove',merge_commit_sha:'c',html_url:'https://example/28',stack:{...f.pulls.b.stack,position:3}};
  if(method==='merge') { f.parents.a=['baseline','head-a']; f.parents.b=['a','head-b']; f.parents.c=['b','head-c']; }
  else f.parents.c=['b'];
  const result=await run(f,['a','b','c']);
  assert.deepEqual(result.map(({tag,sha,pr})=>({tag,sha,pr})),[
   {tag:'v0.1.1',sha:'a',pr:26},{tag:'v0.2.0',sha:'b',pr:27},{tag:'v1.0.0',sha:'c',pr:28}]);
  assert.deepEqual([...f.tags.values()],['a','b','c']);
  await run(f,['a','b','c']); assert.equal(f.releases.size,3);
 });
}
function rebaseFixture() {
 const f=fixture();f.pulls.a.commits=2;f.pulls.b.commits=2;
 f.pulls.b.base.ref='feature/lower';f.pulls.b.stack={base:{ref:'main',sha:null},id:2458232,number:32,position:2,size:2};
 f.associations.a0=[f.pulls.a];f.associations.b0=[f.pulls.b];
 f.parents.a0=['baseline'];f.parents.a=['a0'];f.parents.b0=['a'];f.parents.b=['b0'];
 return f;
}
test('rebase stack releases each full PR at its final landed commit',async()=>{
 const f=rebaseFixture(); const result=await run(f,['a0','a','b0','b']);
 assert.deepEqual(result.map(({tag,sha})=>[tag,sha]),[['v0.1.1','a'],['v0.2.0','b']]);
 assert.equal(f.releases.size,2);
});
test('nonfinal rebase commit preserves breaking footer despite chore title',async()=>{
 const f=rebaseFixture();f.pulls.a.title='chore: update';
 f.messages.a0='refactor: format\n\nBREAKING CHANGE: old format removed';
 await run(f,['a0','a','b0','b']);assert.equal(f.tags.get('tags/v1.0.0'),'a');assert.equal(f.tags.get('tags/v1.1.0'),'b');
});
test('partial stack only releases landed merged prefix',async()=>{
 const f=fixture();f.pulls.b.merged_at=null;f.pulls.b.merged=false;
 f.associations.a=[f.pulls.a,f.pulls.b];
 await run(f,['a']);assert.deepEqual([...f.tags.values()],['a']);
});
test('duplicate PR associations do not create ambiguity',async()=>{
 const f=fixture();f.associations.a=[f.pulls.a,{...f.pulls.a}];await run(f);assert.equal(f.releases.size,1);
});
test('conflicting duplicate PR metadata rejects before publishing',async()=>{
 const f=fixture();f.associations.b=[f.pulls.b,{...f.pulls.b,title:'fix: conflicting'}];
 await assert.rejects(run(f,['a','b']),/conflict/i);assert.equal(f.tags.size,0);
});
for (const invalid of ['wrong-trunk','missing-stack','incomplete','interleaved','nonlinear','wrong-parent','count','unmerged','ambiguous']) {
 test(`reject rebase ${invalid} before any writes`,async()=>{
  const f=rebaseFixture();let commits=['a0','a','b0','b'];
  if(invalid==='wrong-trunk')f.pulls.b.stack.base.ref='dev';
  if(invalid==='missing-stack')delete f.pulls.b.stack;
  if(invalid==='incomplete')commits=['a0','a','b0'];
  if(invalid==='interleaved')commits=['a0','b0','a','b'];
  if(invalid==='nonlinear')f.parents.b0.push('side');
  if(invalid==='wrong-parent')f.parents.b0=['unrelated'];
  if(invalid==='count')f.pulls.b.commits=3;
  if(invalid==='unmerged'){f.pulls.b.merged=false;f.pulls.b.merged_at=null;}
  if(invalid==='ambiguous'){f.pulls.c={...f.pulls.b,number:999};f.associations.b0.push(f.pulls.c);}
  await assert.rejects(run(f,commits));assert.equal(f.tags.size,0);assert.equal(f.releases.size,0);
 });
}
test('later tag conflicts are rejected before publishing earlier layers',async()=>{
 const f=fixture();f.tags.set('tags/v0.2.0','wrong');await assert.rejects(run(f,['a','b']),/conflict/i);assert.equal(f.releases.size,0);assert.equal(f.tags.has('tags/v0.1.1'),false);
});
test('rebase stack dry run validates membership without writes',async()=>{
 const f=rebaseFixture();const result=await run(f,['a0','a','b0','b'],true);assert.equal(result.length,2);assert.equal(f.tags.size,0);assert.equal(f.releases.size,0);
});

test('stack nonrelease layer appears in initial notes without a version bump',async()=>{
 const f=fixture();f.pulls.a.title='chore: prepare';f.pulls.b.base.ref='feature/lower';
 f.pulls.b.stack={base:{ref:'main',sha:null},id:2458232,number:32,position:2,size:2};
 await run(f,['a','b']);assert.deepEqual([...f.tags],[['tags/v0.2.0','b']]);
 assert.match(f.releases.get('v0.2.0').body,/chore: prepare.*example\/26/);assert.match(f.releases.get('v0.2.0').body,/feat: add.*example\/27/);
});
test('rebase notes end at final PR commits and begin after prior release',async()=>{
 const f=rebaseFixture();await run(f,['a0','a','b0','b']);
 assert.deepEqual(f.notes,[{owner:'o',repo:'r',tag_name:'v0.2.0',target_commitish:'b',previous_tag_name:'v0.1.1'}]);
});
test('rebase tag-only recovery and later prefix replay preserve exact versions',async()=>{
 const f=rebaseFixture();f.tags.set('tags/v0.1.1','a');await run(f,['a0','a']);
 await run(f,['a0','a','b0','b']);await run(f,['a0','a','b0','b']);
 assert.deepEqual([...f.tags],[['tags/v0.1.1','a'],['tags/v0.2.0','b']]);assert.equal(f.releases.size,2);
});
test('canonical PR metadata errors propagate without publishing any layer',async()=>{
 const f=fixture();const get=f.github.rest.pulls.get;
 f.github.rest.pulls.get=async(p)=>{if(p.pull_number===27)throw Error('Forbidden');return get(p);};
 await assert.rejects(run(f,['a','b']),/Forbidden/);assert.equal(f.tags.size,0);
});
test('canonical stack metadata can qualify an association lacking optional stack fields',async()=>{
 const f=fixture();f.pulls.b.base.ref='feature/lower';f.associations.b=[{...f.pulls.b}];
 f.pulls.b.stack={base:{ref:'main',sha:null},id:2458232,number:32,position:2,size:2};
 await run(f,['a','b']);assert.equal(f.tags.get('tags/v0.2.0'),'b');
});
