'use strict';

const readline = require('node:readline');
const { cancel, closeSession, createSession, rpc } = require('./broker_client');

const MCP_VERSION = '2025-03-26';

function tools() {
  return [
    tool('saccade.system.capabilities', 'Read the live Node Broker, Extension, and session contract.', {}),
    tool('saccade.tabs.list', 'List only tabs leased to this Agent session.', {}),
    tool('saccade.tabs.open', 'Open and lease a tab, or claim one exact newly-created/user-shared tab for this Agent session.', {
      url: { type: 'string', minLength: 1, maxLength: 8192 },
      active: { type: 'boolean' },
      claim: { type: 'string', enum: ['arm', 'confirm', 'shared'] },
      claim_id: { type: 'string', minLength: 1 },
      tab_id: { type: 'string', minLength: 1 },
      browser_instance_id: { type: 'string', minLength: 1, maxLength: 256 },
    }),
    tool('saccade.tabs.close', 'Close one tab leased to this Agent session.', {
      tab_id: { type: 'string', minLength: 1 },
    }, ['tab_id']),
    tool('saccade.truth.read', 'Read full Truth or a delta for exactly one leased tab. Delta mode requires after_revision.', {
      tab_id: { type: 'string', minLength: 1 },
      mode: { type: 'string', enum: ['full', 'delta'] },
      after_revision: { type: 'integer', minimum: 0 },
      min_objects: { type: 'integer', minimum: 1, maximum: 32 },
      timeout_ms: { type: 'integer', minimum: 1, maximum: 30000 },
      query: {
        type: 'object', additionalProperties: false,
        properties: {
          text: { type: 'string', minLength: 1, maxLength: 256 },
          roles: { type: 'array', maxItems: 32, uniqueItems: true, items: { type: 'string' } },
          affordances: { type: 'array', maxItems: 8, uniqueItems: true, items: { type: 'string' } },
          visibility: { type: 'array', maxItems: 4, uniqueItems: true, items: { type: 'string' } },
          object_ids: { type: 'array', maxItems: 32, uniqueItems: true, items: { type: 'string', minLength: 1 } },
          max_objects: { type: 'integer', minimum: 1, maximum: 32 },
        },
      },
    }, ['tab_id', 'mode']),
    tool('saccade.act', 'Execute one current object-addressed Extension software action, or one bounded local reflex loop, in a leased tab.', {
      tab_id: { type: 'string', minLength: 1 },
      document_id: { type: 'string', minLength: 1 },
      basis_revision: { type: 'integer', minimum: 1 },
      object_id: { type: 'string', minLength: 1 },
      operation: { type: 'string', enum: ['click', 'type', 'select', 'upload'] },
      text: { type: 'string', maxLength: 8192 },
      value: { type: 'string', maxLength: 8192 },
      option_object_id: { type: 'string', minLength: 1 },
      file_path: { type: 'string', minLength: 1, maxLength: 4096 },
      file_sha256: { type: 'string', pattern: '^[a-f0-9]{64}$' },
      max_actions: { type: 'integer', minimum: 1, maximum: 1000 },
      start_object_id: { type: 'string', minLength: 1 },
      steps: {
        type: 'array', minItems: 1, maxItems: 32,
        items: {
          type: 'object', additionalProperties: false,
          required: ['object_id'],
          properties: {
            object_id: { type: 'string', minLength: 1 },
            operation: { type: 'string', enum: ['click', 'type', 'select'] },
            text: { type: 'string', maxLength: 8192 },
            value: { type: 'string', maxLength: 8192 },
            option_object_id: { type: 'string', minLength: 1 },
          },
        },
      },
      timeout_ms: { type: 'integer', minimum: 1, maximum: 60000 },
    }, ['tab_id', 'document_id', 'basis_revision']),
  ];
}

function tool(name, description, properties, required = [], anyOf) {
  return {
    name, description,
    inputSchema: { type: 'object', properties, required, additionalProperties: false, ...(anyOf ? { anyOf } : {}) },
  };
}

function methodForTool(name) {
  const methods = {
    'saccade.system.capabilities': 'system.capabilities',
    'saccade.tabs.list': 'tabs.list',
    'saccade.tabs.open': 'tabs.open',
    'saccade.tabs.close': 'tabs.close',
    'saccade.truth.read': 'truth.read',
    'saccade.act': 'act',
  };
  return methods[name];
}

function write(output, id, result, error) {
  const data = error ? {
    code: error.code,
    stage: error.stage,
    elapsed_ms: error.elapsed_ms,
    retry_safe: error.retry_safe,
    current_revision: error.current_revision,
    outcome: error.outcome,
  } : undefined;
  output.write(`${JSON.stringify(error
    ? { jsonrpc: '2.0', id, error: { code: -32000, message: error.message, data } }
    : { jsonrpc: '2.0', id, result })}\n`);
}

const COMPACT_OBJECT_FIELDS = Object.freeze([
  'object_id', 'object_revision', 'frame', 'kind', 'role', 'name', 'text',
  'description', 'affordances', 'state', 'protected', 'document_bounds_xywh',
  'viewport_bounds_xywh', 'visibility', 'transition', 'actionable',
  'continuous', 'extra',
]);

function boundsRow(bounds) {
  if (!bounds || typeof bounds !== 'object') return null;
  return [bounds.x, bounds.y, bounds.width, bounds.height];
}

function compactTruthForAgent(result) {
  const frameIndexes = new Map((result.frames || []).map((frame, index) => [frame.frame_id, index]));
  const common = new Set([
    'object_id', 'object_revision', 'frame_id', 'kind', 'role', 'name', 'text',
    'description', 'affordances', 'state', 'protected', 'document_bounds',
    'viewport_bounds', 'visibility', 'transition', 'action_token', 'loop_class_token',
  ]);
  const objects = (result.objects || []).map((object) => {
    const extra = Object.fromEntries(Object.entries(object).filter(([key]) => !common.has(key)));
    return [
      object.object_id, object.object_revision, frameIndexes.get(object.frame_id) ?? object.frame_id,
      object.kind, object.role, object.name ?? null, object.text ?? null,
      object.description ?? null, object.affordances || [], object.state || {},
      object.protected === true, boundsRow(object.document_bounds),
      boundsRow(object.viewport_bounds), object.visibility, object.transition,
      typeof object.action_token === 'string', typeof object.loop_class_token === 'string',
      Object.keys(extra).length ? extra : null,
    ];
  });
  const changes = (result.changes || []).map((change) => [
    change.kind, change.object_id, change.object_revision,
  ]);
  return {
    schema: result.schema,
    encoding: 'compact_rows/1',
    tab_id: result.tab_id,
    document_id: result.document_id,
    revision: result.revision,
    mode: result.mode,
    complete: result.complete,
    next_basis_revision: result.next_basis_revision,
    ...(result.base_revision !== undefined ? { base_revision: result.base_revision } : {}),
    ...(result.reset_required !== undefined ? { reset_required: result.reset_required } : {}),
    ...(result.timed_out !== undefined ? { timed_out: result.timed_out } : {}),
    ...(result.match_count !== undefined ? { match_count: result.match_count } : {}),
    ...(result.working_set !== undefined ? { working_set: result.working_set } : {}),
    ...(result.catalog !== undefined ? { catalog: result.catalog } : {}),
    ...(result.object_count !== undefined ? { object_count: result.object_count } : {}),
    frames: result.frames || [],
    geometry: result.geometry,
    object_fields: COMPACT_OBJECT_FIELDS,
    objects,
    change_fields: ['kind', 'object_id', 'object_revision'],
    changes,
    coverage: result.coverage,
    limitations: result.limitations || [],
    gap: result.gap === true,
  };
}

function agentResult(result) {
  if (result?.schema === 'saccade.agent-truth/2') {
    return compactTruthForAgent(result);
  }
  return result;
}

function agentText(result) {
  return JSON.stringify(agentResult(result));
}

async function serveMcp({ input = process.stdin, output = process.stdout } = {}) {
  const session = await createSession();
  const agentSessionId = session.agent_session_id;
  const lines = readline.createInterface({ input, crlfDelay: Infinity });
  const inFlight = new Set();
  const cancelled = new Set();
  const keyFor = (id) => `${typeof id}:${String(id)}`;

  const handle = async (request) => {
    if (request.method === 'notifications/cancelled') {
      const requestId = request.params?.requestId;
      cancelled.add(keyFor(requestId));
      await cancel(session, requestId).catch(() => null);
      return;
    }
    if (request.id === undefined) return;
    const key = keyFor(request.id);
    try {
      if (request.method === 'initialize') {
        write(output, request.id, {
          protocolVersion: MCP_VERSION,
          capabilities: { tools: { listChanged: false } },
          serverInfo: { name: 'saccade-node', version: '0.2.1' },
          instructions: `This MCP session is ${agentSessionId}. Every browser operation requires an exact leased tab_id. Choose truth.read mode full or delta deliberately.`,
        });
      } else if (request.method === 'ping') {
        write(output, request.id, {});
      } else if (request.method === 'tools/list') {
        write(output, request.id, { tools: tools() });
      } else if (request.method === 'tools/call') {
        const name = request.params?.name;
        const method = methodForTool(name);
        if (!method) throw Object.assign(new Error('tool is not registered'), { code: 'METHOD_UNKNOWN' });
        const args = request.params?.arguments || {};
        const timeoutMs = Number.isSafeInteger(args.timeout_ms) ? args.timeout_ms : method === 'tabs.open' ? 25_000 : 10_000;
        const result = await rpc(session, method, args, timeoutMs, request.id);
        if (!cancelled.has(key)) {
          const projected = agentResult(result);
          write(output, request.id, {
            content: [{ type: 'text', text: JSON.stringify(projected) }],
            structuredContent: projected,
          });
        }
      } else {
        throw Object.assign(new Error(`unsupported MCP method ${request.method}`), { code: 'METHOD_UNKNOWN' });
      }
    } catch (error) {
      if (!cancelled.has(key)) write(output, request.id, null, error);
    } finally {
      cancelled.delete(key);
    }
  };
  try {
    for await (const line of lines) {
      if (!line.trim()) continue;
      let request;
      try { request = JSON.parse(line); }
      catch (error) { write(output, null, null, error); continue; }
      const task = handle(request);
      inFlight.add(task);
      task.finally(() => inFlight.delete(task));
    }
    await Promise.allSettled([...inFlight]);
  } finally {
    await closeSession(session);
  }
}

module.exports = {
  MCP_VERSION, agentResult, agentText, compactTruthForAgent, methodForTool, serveMcp, tools,
};
