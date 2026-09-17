'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const { BrokerState } = require('../src/broker');
const { readSceneOrigins } = require('../src/scene_access');

function connectTestConsumer(broker, payload) {
  const connected = broker.connectExtension(payload);
  const connection = broker.connections.get(connected.connection_id);
  connection.last_poll_at = broker.now();
  return connection;
}

function sceneTruth({ tabId = '7', documentId = 'document-1', frameUrl = 'https://owner.example', sceneObjectId = 'scene-1', sceneGeneration = 'scene-gen-1', frameId = 1 } = {}) {
  return {
    schema: 'saccade.observation/1',
    browser_instance_id: 'browser-1',
    tab_id: tabId,
    document_id: documentId,
    revision: 1,
    viewport_revision: 1,
    objects: [{
      object_id: sceneObjectId,
      role: 'scene_object',
      source: 'application_reported',
      scene_generation: sceneGeneration,
      frame_id: frameId,
      name: 'Scene target',
      affordances: ['screenshot'],
      state: {},
    }],
    changes: [],
    frames: [{
      frame_id: frameId,
      document_id: documentId,
      document_url: frameUrl,
    }],
    limitations: [],
  };
}

function writePolicy(pathname, origins) {
  fs.writeFileSync(pathname, JSON.stringify({ schema: 'saccade.scene-access/1', origins }, null, 2), 'utf8');
}

function writeRawPolicy(pathname, contents) {
  fs.writeFileSync(pathname, contents, 'utf8');
}

test('scene access policy parser keeps explicit exact origins, rejects wildcards/paths/credentials/oversize/symlink/invalid config', (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-scene-access-policy-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const policy = path.join(directory, 'scene-access.json');
  const oversized = path.join(directory, 'oversized.json');
  const linked = path.join(directory, 'linked-policy.json');
  const target = path.join(directory, 'target.json');
  const target2 = path.join(directory, 'target-two.json');

  assert.deepEqual(readSceneOrigins(undefined), []);
  assert.deepEqual(readSceneOrigins(path.join(directory, 'missing.json')), []);
  assert.deepEqual(readSceneOrigins(linked), []);

  writePolicy(policy, ['https://owner.example', 'https://owner.example']);
  assert.deepEqual(readSceneOrigins(policy), ['https://owner.example']);

  writePolicy(policy, ['https://owner.example/path']);
  assert.deepEqual(readSceneOrigins(policy), []);

  writePolicy(policy, ['https://user:pass@owner.example']);
  assert.deepEqual(readSceneOrigins(policy), []);

  writePolicy(policy, ['https://*.owner.example']);
  assert.deepEqual(readSceneOrigins(policy), []);

  writePolicy(policy, ['https://owner.example']);
  writeRawPolicy(oversized, JSON.stringify({ schema: 'saccade.scene-access/1', origins: ['https://owner.example'] }).padEnd(5005, '!'));
  assert.deepEqual(readSceneOrigins(oversized), []);

  writePolicy(target, ['https://owner.example']);
  fs.symlinkSync(target, linked);
  assert.deepEqual(readSceneOrigins(linked), []);
  writeRawPolicy(path.join(directory, 'bad-schema.json'), JSON.stringify({ schema: 'wrong-schema', origins: ['https://owner.example'] }));
  assert.deepEqual(readSceneOrigins(path.join(directory, 'bad-schema.json')), []);

  writeRawPolicy(target2, '[this is invalid json');
  assert.deepEqual(readSceneOrigins(target2), []);
});

test('visual authorization prepare uses owner scene policy only with exact leased scene object and current policy file', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-scene-access-authorize-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const policy = path.join(directory, 'scene-access.json');
  const origin = 'https://owner.example';

  const broker = new BrokerState({ scenePolicyPath: policy });
  const owner = broker.createSession().agent_session_id;
  const other = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, {
    browser_instance_id: 'browser-1',
    scene_access_version: 1,
    visual_consent_version: 1,
  });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', sceneTruth({ frameUrl: origin }));
  const scope = { tab_id: '7', document_id: 'document-1', scene_object_id: 'scene-1', scene_generation: 'scene-gen-1' };

  const initialAuthorize = broker.rpc(owner, 'visual.authorization.prepare', scope, 1000);
  const [initialCommand] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(initialCommand.kind, 'visual.authorization.prepare');
  broker.acceptExtensionEvents(connection.connection_id, [{ kind: 'response', command_id: initialCommand.command_id, result: { granted: false } }]);
  assert.deepEqual(await initialAuthorize, { granted: false });

  writePolicy(policy, [origin]);
  assert.deepEqual(await broker.rpc(owner, 'visual.authorization.prepare', scope, 1000), {
    granted: true,
    access_kind: 'owner_scene_policy',
  });

  writePolicy(policy, []);
  const revokedAuthorize = broker.rpc(owner, 'visual.authorization.prepare', scope, 1000);
  const [revokedCommand] = await broker.pollCommands(connection.connection_id, 10);
  assert.equal(revokedCommand.kind, 'visual.authorization.prepare');
  assert.equal(revokedCommand.payload.owner_scene_origin, undefined);
  broker.acceptExtensionEvents(connection.connection_id, [{ kind: 'response', command_id: revokedCommand.command_id, result: { granted: false } }]);
  assert.deepEqual(await revokedAuthorize, { granted: false });

  await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', { ...scope, scene_generation: 'mismatch' }, 1000), { code: 'SCENE_STALE' });
  await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', { ...scope, scene_object_id: 'other-scene' }, 1000), { code: 'SCENE_STALE' });
  await assert.rejects(broker.rpc(other, 'visual.authorization.prepare', scope, 1000), { code: 'TAB_LEASED_ELSEWHERE' });

  writePolicy(policy, [origin]);
  connectTestConsumer(broker, { browser_instance_id: 'browser-1', visual_consent_version: 1, scene_access_version: 0 });
  await assert.rejects(broker.rpc(owner, 'visual.authorization.prepare', scope, 1000), { code: 'SCENE_ACCESS_UPGRADE_REQUIRED' });
});

test('scene reads inject owner_scene_origin only for sequence and are revalidated per request', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-scene-access-read-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const policy = path.join(directory, 'scene-access.json');
  const origin = 'https://owner.example';
  const broker = new BrokerState({ scenePolicyPath: policy });
  const owner = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1', scene_access_version: 1, scene_version: 1 });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', sceneTruth({ frameUrl: origin }));
  writePolicy(policy, [origin]);

  const sequence = {
    tab_id: '7',
    document_id: 'document-1',
    mode: 'sequence',
    object_id: 'scene-1',
    scene_generation: 'scene-gen-1',
  };
  const request = broker.rpc(owner, 'visual.read', sequence, 1000);
  const [sequenceCommand] = await broker.pollCommands(connection.connection_id || connection, 10);
  assert.equal(sequenceCommand.payload.owner_scene_origin, origin);
  const sequenceResult = await (async () => {
    broker.acceptExtensionEvents(connection.connection_id || connection, [{
      kind: 'response',
      command_id: sequenceCommand.command_id,
      result: {
        schema: 'saccade.visual-sequence/1',
        tab_id: '7',
        document_id: 'document-1',
        object_id: 'scene-1',
        scene_generation: 'scene-gen-1',
        frames: [{
          frame_id: 1,
          elapsed_ms: 16,
          simulation_time_ms: 16,
          image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' },
        }],
      },
    }]);
    return request;
  })();
  assert.equal((await sequenceResult).frames[0].frame_id, 1);

  const snapshot = {
    tab_id: '7',
    document_id: 'document-1',
  };
  const pending = broker.rpc(owner, 'visual.read', snapshot, 1000);
  const [snapshotCommand] = await broker.pollCommands(connection.connection_id || connection, 10);
  assert.equal(snapshotCommand.payload.owner_scene_origin, undefined);
  broker.acceptExtensionEvents(connection.connection_id || connection, [{
    kind: 'response',
    command_id: snapshotCommand.command_id,
    result: {
      schema: 'saccade.visual/1',
      tab_id: '7',
      document_id: 'document-1',
      image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' },
    },
  }]);
  await pending;
});

test('scene policy can be removed between request and response and stale owner access is rejected', async (t) => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-scene-access-revoke-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const policy = path.join(directory, 'scene-access.json');
  const origin = 'https://owner.example';
  const broker = new BrokerState({ scenePolicyPath: policy });
  const owner = broker.createSession().agent_session_id;
  const connection = connectTestConsumer(broker, { browser_instance_id: 'browser-1', scene_access_version: 1, scene_version: 1 });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', sceneTruth({ frameUrl: origin }));
  writePolicy(policy, [origin]);

  const request = broker.rpc(owner, 'visual.read', {
    tab_id: '7',
    document_id: 'document-1',
    mode: 'sequence',
    object_id: 'scene-1',
    scene_generation: 'scene-gen-1',
  }, 1000);
  const [command] = await broker.pollCommands(connection.connection_id || connection, 10);
  writePolicy(policy, []);
  broker.acceptExtensionEvents(connection.connection_id || connection, [{
    kind: 'response',
    command_id: command.command_id,
    result: {
      schema: 'saccade.visual-sequence/1',
      tab_id: '7',
      document_id: 'document-1',
      object_id: 'scene-1',
      scene_generation: 'scene-gen-1',
      frames: [{ frame_id: 1, elapsed_ms: 16, simulation_time_ms: 16, image: { type: 'image', mimeType: 'image/webp', data: 'YQ==' } }],
    },
  }]);
  await assert.rejects(request, { code: 'SCENE_ACCESS_REVOKED' });
});

test('public visual.read requests cannot inject owner_scene_origin and still require ordinary scope', async () => {
  const broker = new BrokerState();
  const owner = broker.createSession().agent_session_id;
  connectTestConsumer(broker, { browser_instance_id: 'browser-1' });
  broker.leaseTab('7', owner, { browser_instance_id: 'browser-1' });
  broker.acceptTruth('observation', { ...sceneTruth(), objects: [{
    object_id: 'object-1',
    role: 'button',
    name: 'Tap',
    affordances: ['click'],
    action_token: 'token-1',
  }] });
  await assert.rejects(broker.rpc(owner, 'visual.read', {
    tab_id: '7',
    document_id: 'document-1',
    owner_scene_origin: false,
  }, 1000), { code: 'INVALID_REQUEST' });
});

test('owner scene snapshot requires v2, returns one canvas frame and rechecks revocation', async t => {
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'saccade-scene-still-'));
  t.after(()=>fs.rmSync(directory,{recursive:true,force:true}));
  const policy=path.join(directory,'scene-access.json');writePolicy(policy,['https://owner.example']);
  const broker=new BrokerState({scenePolicyPath:policy}),owner=broker.createSession().agent_session_id;
  connectTestConsumer(broker,{browser_instance_id:'browser-1',scene_version:1,scene_access_version:1});
  broker.leaseTab('7',owner,{browser_instance_id:'browser-1'});broker.acceptTruth('observation',sceneTruth());
  const args={tab_id:'7',document_id:'document-1',object_id:'scene-1',scene_generation:'scene-gen-1',mode:'snapshot'};
  const prepare={tab_id:'7',document_id:'document-1',scene_object_id:'scene-1',scene_generation:'scene-gen-1',scene_mode:'snapshot'};
  await assert.rejects(broker.rpc(owner,'visual.authorization.prepare',prepare),{code:'SCENE_ACCESS_UPGRADE_REQUIRED'});
  const c=connectTestConsumer(broker,{browser_instance_id:'browser-1',scene_version:1,scene_access_version:2});
  broker.acceptTruth('observation',sceneTruth());
  assert.equal((await broker.rpc(owner,'visual.authorization.prepare',prepare)).granted,true);
  for(const revoke of [false,true]) {
    const p=broker.rpc(owner,'visual.read',args,1000);const [cmd]=await broker.pollCommands(c.connection_id,10);
    assert.equal(cmd.payload.owner_scene_origin,'https://owner.example');assert.equal(cmd.payload.mode,'snapshot');
    if(revoke)writePolicy(policy,[]);
    broker.acceptExtensionEvents(c.connection_id,[{kind:'response',command_id:cmd.command_id,result:{schema:'saccade.visual/1',...args,
      method:'application_canvas',frame_id:23,simulation_time_ms:900,image:{type:'image',mimeType:'image/webp',data:'YQ=='}}}]);
    if(revoke)await assert.rejects(p,{code:'SCENE_ACCESS_REVOKED'});else assert.equal((await p).frame_id,23);
  }
});
