import test from 'node:test';
import assert from 'node:assert/strict';

test('security reconciliation paginates, deduplicates and delegates without disclosing alerts', async () => {
  const oldFetch=globalThis.fetch;
  const saved={...process.env};
  const calls=[];
  let issueNumber=20;
  const response=(value) => ({
    ok:true,status:200,json:async()=>value
  });
  try {
    process.env.GITHUB_REPOSITORY='Avkroken/example';
    process.env.GH_TOKEN='test-only';
    process.env.GITHUB_EVENT_NAME='schedule';
    globalThis.fetch=async (url,options) => {
      const u=new URL(url);
      const path=u.pathname;
      const method=options.method;
      const payload=options.body ? JSON.parse(options.body):null;
      calls.push({path,method,payload});
      if (path==='/repos/Avkroken/example') return response({private:false,default_branch:'main'});
      if (path.endsWith('/issues') && method==='GET') return response([
        {number:3,state:'open',body:'<!-- skvallerbyttan-alert:code-scanning:1 -->',assignees:[{login:'Avkroken'}]}
      ]);
      if (path.endsWith('/issues') && method==='POST') return response({
        number:issueNumber++,state:'open',body:payload.body,assignees:[{login:'Avkroken'}]
      });
      if (path.endsWith('/code-scanning/alerts')) return response([{number:1},{number:2}]);
      if (path.endsWith('/dependabot/alerts')) return response([{number:7}]);
      if (path.endsWith('/pulls')) return response([]);
      if (path.endsWith('/assignees')) return response([]);
      throw Error('Unexpected '+method+' '+path);
    };
    await import('./security-reconcile.mjs?test=1');
    const created=calls.filter(c=>c.path.endsWith('/issues')&&c.method==='POST');
    assert.equal(created.length,2, 'old CodeQL alert must not create a duplicate');
    assert.ok(created.every(c=>c.payload.body.includes('Never copy secrets')));
    assert.ok(created.some(c=>c.payload.body.includes('code-scanning:2')));
    assert.ok(created.some(c=>c.payload.body.includes('dependabot:7')));
    assert.ok(calls.some(c=>c.payload?.agent_assignment), 'agent delegation is part of the same run');
    assert.equal(calls.filter(c=>c.path.endsWith('/pulls')&&c.method==='POST').length,0);
    assert.ok(!calls.some(c=>c.path.endsWith('/secret-scanning/alerts')), 'public secret scanning must stay private');
  } finally {
    globalThis.fetch=oldFetch;
    for (const k of ['GITHUB_REPOSITORY','GH_TOKEN','GITHUB_EVENT_NAME']) {
      if(saved[k]===undefined) delete process.env[k]; else process.env[k]=saved[k];
    }
  }
});
