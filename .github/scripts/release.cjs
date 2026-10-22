const {parseTitle, parseMessage, bumpVersion} = require('./pr-policy.cjs');

async function publishReleases({github, owner, repo, commits, dryRun = false}) {
  const scope = {owner, repo};
  const planned = [];
  let version = '0.1.0';
  let previousTag;
  let remaining = commits;
  const releases = await github.paginate(github.rest.repos.listReleases, {...scope, per_page: 100});
  const published = releases.filter(r => !r.draft && !r.prerelease && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(r.tag_name));
  published.sort((a, b) => {
    const av = a.tag_name.slice(1).split('.').map(Number);
    const bv = b.tag_name.slice(1).split('.').map(Number);
    return bv[0] - av[0] || bv[1] - av[1] || bv[2] - av[2];
  });
  if (published.length) {
    previousTag = published[0].tag_name;
    version = previousTag.slice(1);
    const reference = (await github.rest.git.getRef({...scope, ref: `tags/${previousTag}`})).data;
    const index = commits.indexOf(reference.object.sha);
    if (index < 0 || reference.object.type !== 'commit') throw new Error(`Release history conflict: ${previousTag}`);
    remaining = commits.slice(index + 1);
  }
  const groups = await mergedPullRequests(github, scope, remaining, commits);
  const bootstrapNotes = [];
  const pending = [];
  for (const {pr, sha, messages} of groups) {
    parseTitle(pr.title);
    const bumps = messages.map(message => parseMessage(`${pr.title}\n${message.split('\n').slice(1).join('\n')}`).bump);
    const bump = ['major', 'minor', 'patch'].find(candidate => bumps.includes(candidate));
    bootstrapNotes.push(`${pr.title} (${pr.html_url})`);
    if (!bump) continue;
    version = bumpVersion(version, bump);
    const tag = `v${version}`;
    const reference = await optional(() => github.rest.git.getRef({...scope, ref: `tags/${tag}`}));
    if (reference && (reference.object.type !== 'commit' || reference.object.sha !== sha)) {
      throw new Error(`Tag conflict: ${tag} does not point to ${sha}`);
    }
    const release = await optional(() => github.rest.repos.getReleaseByTag({...scope, tag}));
    if (release && (!reference || release.draft || release.prerelease || release.tag_name !== tag)) {
      throw new Error(`Release conflict: ${tag}`);
    }
    planned.push({tag, sha, pr: pr.number, existing: Boolean(release)});
    pending.push({tag, sha, reference, release, previousTag, bootstrapNotes: [...bootstrapNotes]});
    previousTag = tag;
  }
  for (const {tag, sha, reference, release, previousTag, bootstrapNotes} of pending) {
    if (release || dryRun) continue;
    const body = previousTag
      ? (await github.rest.repos.generateReleaseNotes({...scope, tag_name: tag, target_commitish: sha, previous_tag_name: previousTag})).data.body
      : `Initial release since the CI release baseline.\n\n${bootstrapNotes.join('\n')}`;
    if (!reference) await github.rest.git.createRef({...scope, ref: `refs/tags/${tag}`, sha});
    await github.rest.repos.createRelease({...scope, tag_name: tag, target_commitish: sha, name: tag, body, draft: false, prerelease: false});
  }
  return planned;
}

async function mergedPullRequests(github, scope, remaining, history) {
  const historyIndices = new Map(history.map((sha, index) => [sha, index]));
  const canonical = new Map();
  const observed = new Map();
  const groups = [];
  const completed = new Set();
  for (const sha of remaining) {
    const pulls = await github.paginate(github.rest.repos.listPullRequestsAssociatedWithCommit, {...scope, commit_sha: sha, per_page: 100});
    const unique = new Map();
    for (const pr of pulls) {
      const metadata = JSON.stringify([pr.title, pr.merged_at, pr.merge_commit_sha, pr.base?.ref, pr.stack?.base?.ref]);
      if (observed.has(pr.number) && observed.get(pr.number) !== metadata) throw new Error(`Conflicting metadata for PR #${pr.number}`);
      observed.set(pr.number, metadata);
      unique.set(pr.number, pr);
    }
    const eligible = [];
    for (const pr of unique.values()) {
      if (!pr.merged_at) continue;
      if (!canonical.has(pr.number)) {
        canonical.set(pr.number, (await github.rest.pulls.get({...scope, pull_number: pr.number})).data);
      }
      const detail = canonical.get(pr.number);
      if (detail.number !== pr.number || detail.title !== pr.title || detail.merged_at !== pr.merged_at || detail.merge_commit_sha !== pr.merge_commit_sha || detail.base?.ref !== pr.base?.ref || (pr.stack && detail.stack?.base?.ref !== pr.stack.base?.ref)) {
        throw new Error(`Conflicting metadata for PR #${pr.number}`);
      }
      if (detail.merged && (detail.stack ? detail.stack.base?.ref : detail.base?.ref) === 'main') eligible.push(detail);
    }
    if (eligible.length !== 1) throw new Error(`Expected one merged main PR for ${sha}, found ${eligible.length}`);
    const pr = eligible[0];
    let group = groups.at(-1);
    if (!group || group.pr.number !== pr.number) {
      if (group && group.sha !== group.pr.merge_commit_sha) throw new Error(`Incomplete landed commits for PR #${group.pr.number}`);
      if (completed.has(pr.number)) throw new Error(`Interleaved landed commits for PR #${pr.number}`);
      completed.add(pr.number);
      group = {pr, sha, commits: [], messages: []};
      groups.push(group);
    }
    const commit = (await github.rest.repos.getCommit({...scope, ref: sha})).data;
    group.sha = sha;
    group.commits.push({sha, parents: commit.parents});
    group.messages.push(commit.commit.message);
  }
  for (const group of groups) {
    if (group.sha !== group.pr.merge_commit_sha) throw new Error(`Incomplete landed commits for PR #${group.pr.number}`);
    if (group.commits.length === 1) continue;
    if (group.commits.length !== group.pr.commits) throw new Error(`Incomplete rebase membership for PR #${group.pr.number}`);
    for (const {sha, parents} of group.commits) {
      const index = historyIndices.get(sha);
      if (parents?.length !== 1 || (index > 0 && parents[0].sha !== history[index - 1])) {
        throw new Error(`Nonlinear rebase membership for PR #${group.pr.number} at ${sha}`);
      }
    }
  }
  return groups;
}

async function optional(request) {
  try { return (await request()).data; }
  catch (error) { if (error.status === 404) return null; throw error; }
}

module.exports = {publishReleases};
