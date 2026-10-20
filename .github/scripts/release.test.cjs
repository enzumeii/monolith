const {test}=require('node:test');
const assert=require('node:assert/strict');
const {publishReleases}=require('./release.cjs');
function fixture() {
 const tags=new Map(), releases=new Map();
 const pulls={a:{number:26,title:'ci: finalize',merged_at:'2026-10-20',merge_commit_sha:'a',base:{ref:'main'},html_url:'https://example/26'},b:{number:27,title:'chore: update',merged_at:'2026-10-21',merge_commit_sha:'b',base:{ref:'main'},html_url:'https://example/27'}};
 const missing=()=>{throw Object.assign(new Error('Not found'),{status:404});};
 const github={rest:{repos:{
 listReleases:async()=>({data:[...releases.values()]}),
 listPullRequestsAssociatedWithCommit:async({commit_sha})=>({data:pulls[commit_sha]?[pulls[commit_sha]]:[]}),
 getReleaseByTag:async({tag})=>({data:releases.get(tag)||missing()}),
 generateReleaseNotes:async()=>({data:{body:'Generated notes'}}),
 createRelease:async(p)=>{releases.set(p.tag_name,p);return {data:p};}
 },git:{
 getRef:async({ref})=>({data:{object:{sha:tags.get(ref)||missing(),type:'commit'}}}),
 createRef:async({ref,sha})=>{tags.set(ref.replace('refs/',''),sha);}
 }},paginate:async(fn,p)=>(await fn(p)).data};
 return {github,tags,releases,pulls};
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
