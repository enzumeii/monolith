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
  const bootstrapNotes = [];
  for (const sha of remaining) {
    const pulls = await github.paginate(github.rest.repos.listPullRequestsAssociatedWithCommit, {...scope, commit_sha: sha, per_page: 100});
    const merged = pulls.filter(pr => pr.merged_at && pr.base.ref === 'main' && pr.merge_commit_sha === sha);
    if (merged.length !== 1) throw new Error(`Expected one merged main PR for ${sha}, found ${merged.length}`);
    const pr = merged[0];
    parseTitle(pr.title);
    const message = (await github.rest.repos.getCommit({...scope, ref: sha})).data.commit.message;
    const body = message.split('\n').slice(1).join('\n');
    const {bump} = parseMessage(`${pr.title}\n${body}`);
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
    if (!release && !dryRun) {
      const body = previousTag
        ? (await github.rest.repos.generateReleaseNotes({...scope, tag_name: tag, target_commitish: sha, previous_tag_name: previousTag})).data.body
        : `Initial release since the CI release baseline.\n\n${bootstrapNotes.join('\n')}`;
      if (!reference) await github.rest.git.createRef({...scope, ref: `refs/tags/${tag}`, sha});
      await github.rest.repos.createRelease({...scope, tag_name: tag, target_commitish: sha, name: tag, body, draft: false, prerelease: false});
    }
    previousTag = tag;
  }
  return planned;
}

async function optional(request) {
  try { return (await request()).data; }
  catch (error) { if (error.status === 404) return null; throw error; }
}

module.exports = {publishReleases};
