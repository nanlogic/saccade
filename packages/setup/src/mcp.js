'use strict';

const readline = require('node:readline');
const { cancel, closeSession, createSession, rpc } = require('./broker_client');
const { createMediaAuthorization, createSessionObservationConsent } = require('./media_authorization');

const MCP_VERSION = '2025-06-18';

function tools() {
  return [
    tool('saccade.system.capabilities', 'Read the live Node Broker, Extension, and session contract.', {}),
    tool('saccade.tabs.list', 'List only tabs leased to this Agent session.', {}),
    tool('saccade.tabs.open', 'Open and lease a tab. To connect an existing private tab, use claim: request with a short connection_label describing this task and browser_instance_id from capabilities; no tab_id or URL. Show the returned label/code to the user, who clicks Allow in the target tab popup, then call tabs.list. Never ask the user to guess a session suffix. Requests expire after two minutes and grant no access until approved. Legacy exact-tab claims remain available.', {
      url: { type: 'string', minLength: 1, maxLength: 8192 },
      active: { type: 'boolean' },
      claim: { type: 'string', enum: ['arm', 'confirm', 'shared', 'request'] },
      connection_label: { type: 'string', minLength: 1, maxLength: 80 },
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
    tool('saccade.media.read', 'Read video evidence from one leased tab. For tutorials, first use truth.read and supported page controls to read an available timestamped transcript (including YouTube Show transcript); do not wait for normal or accelerated playback. A missing native caption track does not prove the page has no transcript. Label auto-generated transcript evidence and use its timestamps to target visual checks of steps, parameters and results. Text-only answers must not claim visual verification. For visual research, catalog then sample relevant videos sequentially; do not ask the user to choose extraction modes. Catalog is not complete for unloaded feeds. overview returns up to 8 timestamped frames and existing captions; detail reads at most 3 seconds at 4 fps or one previously observed time_s. Bind current tab, document, object and media_id. Video access reuses unified session observation confirmation, or narrower negotiated host intent/manual permission. Never supply approval arguments or ask again when this session already has consent. For generic page research, explain why video evidence is needed before requesting consent. Unsupported clients return a limitation; do not repeatedly tell users to toggle the popup. Distinguish author claims, captions and visible evidence. Report relevance, visible operations with actual times, uncertainty and 1-3 replay intervals. Say not shown in this sample, never absent from the entire video. If neither captions nor a page transcript was read, report that audio was not analyzed. At most two detail reads for conclusion-changing uncertainty. No automatic replay.', {
      tab_id: { type: 'string', minLength: 1 }, document_id: { type: 'string', minLength: 1 },
      mode: { type: 'string', enum: ['catalog', 'overview', 'detail'] },
      object_id: { type: 'string', minLength: 1 }, media_id: { type: 'string', minLength: 1 },
      start_s: { type: 'number', minimum: 0 }, end_s: { type: 'number', minimum: 0 }, time_s: { type: 'number', minimum: 0 },
      max_width: { type: 'integer', minimum: 320, maximum: 1600 },
      timeout_ms: { type: 'integer', minimum: 1, maximum: 30000 },
    }, ['tab_id', 'document_id', 'mode']),
    tool('saccade.visual.read', 'Read visual evidence in one authorized active tab. For a registered scene_object, mode sequence with exact object_id and scene_generation returns bounded render frames, per-frame application state, and measured-joint summaries when supplied by the application. Owner-configured exact-origin scene access also covers snapshot of that exact scene object: one application-Canvas frame with actual frame metadata, not a composited page screenshot. This scope skips confirmation but grants no viewport screenshot or video access; scene stills require Extension scene_access_version 2. Other captures use negotiated host intent or client confirmation. Do not ask for popup toggles or supply approval arguments. Snapshot object_id crops to a current object. Images and application-reported measurements are evidence, not action authority or proof of unsampled motion; no automatic replay.', {
      mode: { type: 'string', enum: ['snapshot', 'sequence'], description: 'Default snapshot. Experimental sequence reads up to 3 seconds of an application-registered scene object without moving cameras or running actions. Requires scene_version 1; sparse frames are not complete motion coverage.' },
      scene_generation: { type: 'string', minLength: 1, description: 'Exact generation from current scene_object Truth; required for scene snapshots and sequences.' },
      duration_ms: { type: 'integer', minimum: 100, maximum: 3000 },
      tab_id: { type: 'string', minLength: 1 },
      document_id: { type: 'string', minLength: 1 },
      object_id: { type: 'string', minLength: 1 },
      max_width: { type: 'integer', minimum: 320, maximum: 1600 },
      timeout_ms: { type: 'integer', minimum: 1, maximum: 30000, description: 'Execution budget, default 30000 ms, excluding human confirmation. One approval lasts for this live session; Saccade imposes no confirmation countdown. Client cancellation or session close still stops waiting. After consent, one execution deadline covers any fresh document preparation, grant acceptance and capture; snapshot execution remains capped at 10000 ms.' },
    }, ['tab_id', 'document_id']),
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
    annotations: annotationsForTool(name),
    inputSchema: { type: 'object', properties, required, additionalProperties: false, ...(anyOf ? { anyOf } : {}) },
  };
}

function annotationsForTool(name) {
  // Client-facing hints never grant consent or authorize automatic replay.
  // Keep unknown tools conservative until their complete behavior is reviewed.
  switch (name) {
    case 'saccade.system.capabilities':
    case 'saccade.tabs.list':
      return { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };
    case 'saccade.truth.read':
      return { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: true };
    case 'saccade.visual.read':
      // Pixels require separate consent and captures must never be replayed.
      return { readOnlyHint: true, destructiveHint: false, idempotentHint: false, openWorldHint: true };
    case 'saccade.tabs.open':
      return { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true };
    case 'saccade.media.read': // Seeking can interrupt playback; restoration can fail.
    case 'saccade.tabs.close':
    case 'saccade.act':
    default:
      return { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true };
  }
}

function methodForTool(name) {
  const methods = {
    'saccade.system.capabilities': 'system.capabilities',
    'saccade.tabs.list': 'tabs.list',
    'saccade.tabs.open': 'tabs.open',
    'saccade.tabs.close': 'tabs.close',
    'saccade.truth.read': 'truth.read',
    'saccade.act': 'act',
    'saccade.visual.read': 'visual.read',
    'saccade.media.read': 'media.read',
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

function authorizationErrorResult(error) {
  const failure = {
    code: error.code, stage: error.stage, retry_safe: false,
    message: error.message,
  };
  return {
    isError: true,
    content: [{ type: 'text', text: JSON.stringify({ error: failure }) }],
    structuredContent: { error: failure },
  };
}

function toolResult(method, result) {
  if (method === 'media.read' || (method === 'visual.read' && result.schema === 'saccade.visual-sequence/1')) {
    const frames = result.frames || [];
    const metadata = { ...result, frames: frames.map(({ image, ...frame }, index) => ({ ...frame, frame_index: index })) };
    return { structuredContent: metadata, content: [{ type: 'text', text: JSON.stringify(metadata) },
      ...frames.flatMap((frame, index) => [{ type: 'text', text: `Frame ${index}: ${frame.time_s ?? frame.simulation_time_ms/1000}s (${frame.method})` }, frame.image])] };
  }
  const { image, ...metadata } = method === 'visual.read' ? result : {};
  const projected = agentResult(method === 'visual.read' ? metadata : result);
  return {
    content: [{ type: 'text', text: JSON.stringify(projected) }, ...(image ? [image] : [])],
    structuredContent: projected,
  };
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
  const sessionConsent = createSessionObservationConsent();
  const mediaAuthorization = createMediaAuthorization({ output, sessionConsent,
    invoke: (...args) => rpc(session, ...args), isCancelled: (id) => cancelled.has(keyFor(id)) });
  const visualAuthorization = createMediaAuthorization({ output, observation: 'visual', sessionConsent,
    invoke: (...args) => rpc(session, ...args), isCancelled: (id) => cancelled.has(keyFor(id)) });

  const handle = async (request) => {
    if (request.method === 'notifications/cancelled') {
      const requestId = request.params?.requestId;
      cancelled.add(keyFor(requestId));
      mediaAuthorization.cancel(requestId);
      visualAuthorization.cancel(requestId);
      await cancel(session, requestId).catch(() => null);
      return;
    }
    if (request.id === undefined) return;
    const key = keyFor(request.id);
    try {
      if (request.method === 'initialize') {
        mediaAuthorization.initialize(request.params?.protocolVersion === '2025-03-26'
          ? { ...request.params?.capabilities, elicitation: undefined } : request.params?.capabilities);
        visualAuthorization.initialize(request.params?.protocolVersion === '2025-03-26'
          ? { ...request.params?.capabilities, elicitation: undefined } : request.params?.capabilities);
        write(output, request.id, {
          protocolVersion: request.params?.protocolVersion === '2025-03-26' ? '2025-03-26' : MCP_VERSION,
          capabilities: { tools: { listChanged: false } },
          serverInfo: { name: 'saccade-node', version: require('../package.json').version },
          instructions: `This MCP session is ${agentSessionId}. Every browser operation requires an exact leased tab_id. Choose truth.read mode full or delta deliberately. Close temporary tabs you opened when their task is complete; leave user-shared tabs open unless asked to close them. One unified observation confirmation covers screenshots, video and short 3D sequences across this live session's authorized tabs until session close or revocation; do not ask separately for each medium, new tab, refresh or verified reconnect.`,
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
        const observation = method === 'media.read' || method === 'visual.read';
        const timeoutMs = Number.isSafeInteger(args.timeout_ms) ? args.timeout_ms : observation ? 30_000 : method === 'tabs.open' ? 25_000 : 10_000;
        let deadline = Date.now() + Math.min(timeoutMs, 30000);
        if (observation && (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 30000)) throw Object.assign(new Error('Observation execution timeout must be 1..30000 ms'), { code: 'INVALID_REQUEST' });
        if (method === 'visual.read') {
          if (Object.keys(args).some(key => !['tab_id','document_id','object_id','max_width','timeout_ms','mode','scene_generation','duration_ms'].includes(key))) throw Object.assign(new Error('Unexpected image arguments; approval cannot be supplied by the Agent'), { code: 'INVALID_REQUEST' });
          ({ deadline_at: deadline } = await visualAuthorization.ensure(request, deadline, { executionTimeoutMs: timeoutMs }));
        }
        if (method === 'media.read' && ['overview', 'detail'].includes(args.mode)) {
          if (Object.keys(args).some((key) => !['tab_id','document_id','mode','object_id','media_id','start_s','end_s','time_s','max_width','timeout_ms'].includes(key))) {
            throw Object.assign(new Error('Unexpected media arguments; approval cannot be supplied by the Agent'), { code: 'INVALID_REQUEST' });
          }
          ({ deadline_at: deadline } = await mediaAuthorization.ensure(request, deadline, { executionTimeoutMs: timeoutMs }));
        }
        const result = await rpc(session, method, args, observation ? Math.max(1, deadline - Date.now()) : timeoutMs, request.id,
          observation ? deadline : undefined);
        if (method === 'system.capabilities') {
          result.media_client_authorization = mediaAuthorization.describe();
          result.visual_client_authorization = visualAuthorization.describe();
        }
        if (!cancelled.has(key)) {
          write(output, request.id, toolResult(method, result));
        }
      } else {
        throw Object.assign(new Error(`unsupported MCP method ${request.method}`), { code: 'METHOD_UNKNOWN' });
      }
    } catch (error) {
      if (!cancelled.has(key)) {
        // A denied observation is a tool execution failure, not a malformed MCP
        // request. Keep its explanation available to clients as a tool result.
        if (request.method === 'tools/call'
          && ['media_authorization', 'visual_authorization'].includes(error.stage)) {
          await cancel(session, request.id).catch(() => null);
          write(output, request.id, authorizationErrorResult(error));
        } else write(output, request.id, null, error);
      }
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
      if (mediaAuthorization.receive(request) || visualAuthorization.receive(request)) continue;
      // JSON-RPC responses (including expired consent answers) are never new
      // requests. Replying METHOD_UNKNOWN here creates a response-to-response loop.
      if (!request.method && (Object.hasOwn(request, 'result') || Object.hasOwn(request, 'error'))) continue;
      const task = handle(request);
      inFlight.add(task);
      task.finally(() => inFlight.delete(task));
    }
    mediaAuthorization.close();
    visualAuthorization.close();
    await Promise.allSettled([...inFlight]);
  } finally {
    mediaAuthorization.close();
    visualAuthorization.close();
    await closeSession(session);
  }
}

module.exports = {
  MCP_VERSION, agentResult, agentText, annotationsForTool, compactTruthForAgent, methodForTool, serveMcp, tools, toolResult,
};
