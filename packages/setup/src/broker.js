'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { EventEmitter } = require('node:events');
const { WebSocketServer } = require('ws');

const BROKER_SCHEMA = 'saccade.node-broker/1';
const DEFAULT_PORT = 32177;
const MAX_BODY_BYTES = 8 * 1024 * 1024;
const HISTORY_LIMIT = 256;
const DIAGNOSTIC_LIMIT = 256;
const COMMAND_LIMIT = 1024;
const EXTENSION_POLL_HEARTBEAT_MS = 2_000;
const STATE_SCHEMA = 'saccade.node-broker-state/1';
const OCCURRENCE_LIMIT = 256;
const MAX_UPLOAD_BYTES = 16 * 1024 * 1024;
const UPLOAD_MIME_TYPES = new Map([
  ['.avif', 'image/avif'], ['.gif', 'image/gif'], ['.jpeg', 'image/jpeg'],
  ['.jpg', 'image/jpeg'], ['.png', 'image/png'], ['.webp', 'image/webp'],
  ['.mp4', 'video/mp4'], ['.webm', 'video/webm'], ['.mov', 'video/quicktime'],
  ['.pdf', 'application/pdf'], ['.csv', 'text/csv'], ['.json', 'application/json'],
  ['.txt', 'text/plain'], ['.zip', 'application/zip'],
]);

function opaque(prefix) {
  return `${prefix}_${crypto.randomBytes(24).toString('base64url')}`;
}

function cleanId(value, field) {
  if (typeof value !== 'string' || value.length < 1 || value.length > 256
      || /[\u0000-\u001f\u007f]/.test(value)) throw new Error(`${field} is invalid`);
  return value;
}

function cleanBrowserFamily(value) {
  if (!['chrome', 'edge'].includes(value)) throw new Error('browser_family is invalid');
  return value;
}

function cleanExtensionCandidate(value) {
  if (!value || value.schema !== 'saccade.extension-candidate/1'
      || typeof value.id !== 'string' || !/^[a-f0-9]{64}$/.test(value.id)
      || typeof value.version !== 'string' || value.version.length < 1 || value.version.length > 64
      || /[\u0000-\u001f\u007f]/.test(value.version)) {
    throw new Error('extension_candidate is invalid');
  }
  return { schema: value.schema, id: value.id, version: value.version };
}

function boundedTimeout(value, fallback = 10_000) {
  const timeout = Number.isSafeInteger(value) ? value : fallback;
  if (timeout < 1 || timeout > 60_000) throw new Error('timeout_ms must be between 1 and 60000');
  return timeout;
}

function jsonSize(value) {
  return Buffer.byteLength(JSON.stringify(value));
}

function withinPathRoot(candidate, root) {
  const relative = path.relative(root, candidate);
  return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

function normalizeUploadRoots(values) {
  const roots = [];
  for (const value of values || []) {
    try {
      const resolved = fs.realpathSync.native(path.resolve(String(value)));
      if (fs.statSync(resolved).isDirectory() && !roots.includes(resolved)) roots.push(resolved);
    } catch (_error) { /* Missing roots grant no authority. */ }
  }
  return roots;
}

function materializeUploadFile(filePath, expectedSha256, roots) {
  if (typeof filePath !== 'string' || !path.isAbsolute(filePath)
      || filePath.length < 1 || filePath.length > 4096
      || /[\u0000-\u001f\u007f]/.test(filePath)) {
    throw new BrokerError('UPLOAD_PATH_INVALID', 'file_path must be one absolute local path', { retry_safe: true });
  }
  let resolved;
  let stats;
  try {
    const supplied = fs.lstatSync(filePath);
    if (supplied.isSymbolicLink()) {
      throw new BrokerError('UPLOAD_PATH_DENIED', 'Symbolic-link uploads are not allowed', { retry_safe: true });
    }
    resolved = fs.realpathSync.native(filePath);
    stats = fs.statSync(resolved);
  } catch (error) {
    if (error instanceof BrokerError) throw error;
    throw new BrokerError('UPLOAD_FILE_UNAVAILABLE', 'The upload file is unavailable', { retry_safe: true });
  }
  if (!roots.some((root) => withinPathRoot(resolved, root))) {
    throw new BrokerError('UPLOAD_PATH_DENIED', 'The upload file is outside the configured workspace roots', { retry_safe: true });
  }
  if (!stats.isFile()) {
    throw new BrokerError('UPLOAD_FILE_INVALID', 'The upload target must be one regular file', { retry_safe: true });
  }
  if (stats.size < 1 || stats.size > MAX_UPLOAD_BYTES) {
    throw new BrokerError('UPLOAD_FILE_SIZE', `The upload file must be between 1 byte and ${MAX_UPLOAD_BYTES} bytes`, { retry_safe: true });
  }
  let content;
  try { content = fs.readFileSync(resolved); }
  catch (_error) {
    throw new BrokerError('UPLOAD_FILE_UNAVAILABLE', 'The upload file could not be read', { retry_safe: true });
  }
  const sha256 = crypto.createHash('sha256').update(content).digest('hex');
  if (expectedSha256 !== undefined && expectedSha256 !== sha256) {
    throw new BrokerError('UPLOAD_HASH_MISMATCH', 'The upload file changed before dispatch', { retry_safe: true });
  }
  const name = path.basename(resolved);
  if (!name || name.length > 255 || /[\u0000-\u001f\u007f]/.test(name)) {
    throw new BrokerError('UPLOAD_FILE_INVALID', 'The upload filename is invalid', { retry_safe: true });
  }
  return {
    kind: 'file',
    file: {
      name,
      mime_type: UPLOAD_MIME_TYPES.get(path.extname(name).toLowerCase()) || 'application/octet-stream',
      size_bytes: content.length,
      sha256,
      content_base64: content.toString('base64'),
    },
  };
}

function scrubCommandPayload(command) {
  const file = command?.payload?.payload?.file;
  if (file && typeof file.content_base64 === 'string') delete file.content_base64;
}

function defaultStatePath(environment = process.env) {
  const stateDirectory = environment.SACCADE_STATE_DIR || path.join(os.homedir(), '.saccade');
  return path.join(stateDirectory, 'broker-state.json');
}

function resumeHash(token) {
  return crypto.createHash('sha256').update(cleanId(token, 'resume_token')).digest('base64url');
}

function equalHash(left, right) {
  const leftBuffer = Buffer.from(String(left));
  const rightBuffer = Buffer.from(String(right));
  return leftBuffer.length === rightBuffer.length && crypto.timingSafeEqual(leftBuffer, rightBuffer);
}

class BrokerError extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.code = code;
    this.details = details;
    Object.assign(this, details);
  }
}

class BrokerState extends EventEmitter {
  constructor({ now = () => Date.now(), statePath = null, uploadRoots = [process.cwd()] } = {}) {
    super();
    this.now = now;
    this.statePath = statePath;
    this.epoch = opaque('broker');
    this.sessions = new Map();
    this.leases = new Map();
    this.truth = new Map();
    this.connections = new Map();
    this.commands = new Map();
    this.cancelledRequests = new Set();
    this.diagnostics = [];
    this.occurrences = [];
    this.defaultUploadRoots = normalizeUploadRoots(uploadRoots);
    this.loadState();
  }

  loadState() {
    if (!this.statePath || !fs.existsSync(this.statePath)) return;
    let stored;
    try {
      stored = JSON.parse(fs.readFileSync(this.statePath, 'utf8'));
    } catch (error) {
      throw new BrokerError('STATE_CORRUPT', 'Broker recovery state is unreadable', { cause: error.message });
    }
    if (stored.schema !== STATE_SCHEMA || !Array.isArray(stored.sessions)
        || !Array.isArray(stored.leases) || !Array.isArray(stored.occurrences)) {
      throw new BrokerError('STATE_CORRUPT', 'Broker recovery state has an unsupported shape');
    }
    for (const value of stored.sessions.slice(0, COMMAND_LIMIT)) {
      try {
        const agentSessionId = cleanId(value.agent_session_id, 'agent_session_id');
        if (typeof value.resume_token_hash !== 'string' || !value.resume_token_hash) continue;
        this.sessions.set(agentSessionId, {
          agent_session_id: agentSessionId,
          connected_at: value.connected_at,
          last_seen_at: value.last_seen_at,
          resume_token_hash: value.resume_token_hash,
          state: 'recoverable',
        });
      } catch (_error) { /* Ignore malformed bounded entries, never broaden authority. */ }
    }
    for (const value of stored.leases.slice(0, COMMAND_LIMIT)) {
      try {
        const tabId = cleanId(value.tab_id, 'tab_id');
        const agentSessionId = cleanId(value.agent_session_id, 'agent_session_id');
        if (!this.sessions.has(agentSessionId) && value.state !== 'orphaned') continue;
        this.leases.set(tabId, {
          tab_id: tabId,
          agent_session_id: agentSessionId,
          state: value.state === 'orphaned' ? 'orphaned' : 'recoverable',
          leased_at: value.leased_at,
          orphaned_at: value.orphaned_at,
          ownership: value.ownership,
          browser_instance_id: value.browser_instance_id,
        });
      } catch (_error) { /* Ignore malformed bounded entries, never broaden authority. */ }
    }
    this.occurrences = stored.occurrences.slice(-OCCURRENCE_LIMIT).map((value) => (
      value.occurrence === 'dispatched'
        ? { ...value, occurrence: 'outcome_unknown', code: 'BROKER_RESTART', retry_safe: false }
        : value
    ));
  }

  persistState() {
    if (!this.statePath) return;
    const value = {
      schema: STATE_SCHEMA,
      saved_at: this.now(),
      broker_epoch: this.epoch,
      sessions: [...this.sessions.values()]
        .filter((session) => session.resume_token_hash)
        .map((session) => ({
          agent_session_id: session.agent_session_id,
          connected_at: session.connected_at,
          last_seen_at: session.last_seen_at,
          resume_token_hash: session.resume_token_hash,
        })),
      leases: [...this.leases.values()].map((lease) => ({
        tab_id: lease.tab_id,
        agent_session_id: lease.agent_session_id,
        state: lease.state,
        leased_at: lease.leased_at,
        orphaned_at: lease.orphaned_at,
        ownership: lease.ownership,
        browser_instance_id: lease.browser_instance_id,
      })),
      occurrences: this.occurrences.slice(-OCCURRENCE_LIMIT),
    };
    const directory = path.dirname(this.statePath);
    const temporary = `${this.statePath}.${process.pid}.${crypto.randomBytes(6).toString('hex')}.tmp`;
    try {
      fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
      fs.writeFileSync(temporary, `${JSON.stringify(value)}\n`, { mode: 0o600 });
      fs.renameSync(temporary, this.statePath);
    } catch (error) {
      try { fs.unlinkSync(temporary); } catch (_cleanupError) { /* best effort */ }
      throw new BrokerError('STATE_PERSIST_FAILED', 'Broker recovery state could not be persisted', { cause: error.message });
    }
  }

  createSession({ resume_token: resumeProof, upload_root: uploadRoot } = {}) {
    const uploadRoots = uploadRoot === undefined
      ? this.defaultUploadRoots : normalizeUploadRoots([uploadRoot]);
    if (resumeProof !== undefined) {
      const hash = resumeHash(resumeProof);
      const session = [...this.sessions.values()].find((candidate) => (
        candidate.resume_token_hash
        && equalHash(candidate.resume_token_hash, hash)
      ));
      if (!session) throw new BrokerError('RESUME_DENIED', 'Session resume proof is invalid');
      if (session.state === 'online') throw new BrokerError('SESSION_ALREADY_ONLINE', 'Agent session is already online');
      const previousState = session.state;
      const previousHash = session.resume_token_hash;
      const previousUploadRoots = session.upload_roots;
      session.state = 'online';
      session.last_seen_at = this.now();
      session.upload_roots = uploadRoots;
      const rotatedToken = opaque('resume');
      session.resume_token_hash = resumeHash(rotatedToken);
      const resumedLeases = [];
      for (const lease of this.leases.values()) {
        if (lease.agent_session_id === session.agent_session_id && lease.state === 'recoverable') {
          lease.state = 'active';
          resumedLeases.push(lease);
        }
      }
      try { this.persistState(); }
      catch (error) {
        session.state = previousState;
        session.resume_token_hash = previousHash;
        session.upload_roots = previousUploadRoots;
        for (const lease of resumedLeases) lease.state = 'recoverable';
        throw error;
      }
      return {
        agent_session_id: session.agent_session_id,
        broker_epoch: this.epoch,
        resume_token: rotatedToken,
        resumed: true,
        resumed_tabs: resumedLeases.length,
      };
    }
    const agentSessionId = opaque('agent');
    const resumeToken = opaque('resume');
    this.sessions.set(agentSessionId, {
      agent_session_id: agentSessionId,
      connected_at: this.now(),
      last_seen_at: this.now(),
      resume_token_hash: resumeHash(resumeToken),
      upload_roots: uploadRoots,
      state: 'online',
    });
    try { this.persistState(); }
    catch (error) { this.sessions.delete(agentSessionId); throw error; }
    return {
      agent_session_id: agentSessionId,
      broker_epoch: this.epoch,
      resume_token: resumeToken,
      resumed: false,
      resumed_tabs: 0,
    };
  }

  touchSession(agentSessionId) {
    const session = this.sessions.get(cleanId(agentSessionId, 'agent_session_id'));
    if (!session || session.state !== 'online') throw new BrokerError('SESSION_OFFLINE', 'Agent session is not online');
    session.last_seen_at = this.now();
    return session;
  }

  authorizeSession(agentSessionId, resumeProof) {
    const session = this.sessions.get(cleanId(agentSessionId, 'agent_session_id'));
    if (!session || !session.resume_token_hash
        || typeof resumeProof !== 'string' || !equalHash(session.resume_token_hash, resumeHash(resumeProof))) {
      throw new BrokerError('SESSION_AUTH_FAILED', 'Agent session proof is invalid');
    }
    if (session.state !== 'online') throw new BrokerError('SESSION_OFFLINE', 'Agent session requires recovery');
    return session;
  }

  closeSession(agentSessionId) {
    const session = this.sessions.get(agentSessionId);
    if (!session) return { orphaned_tabs: 0 };
    session.state = 'offline';
    session.resume_token_hash = null;
    session.upload_roots = [];
    let orphaned = 0;
    for (const lease of this.leases.values()) {
      if (lease.agent_session_id === agentSessionId && lease.state === 'active') {
        lease.state = 'orphaned';
        lease.orphaned_at = this.now();
        orphaned += 1;
      }
    }
    for (const command of this.commands.values()) {
      if (command.agent_session_id !== agentSessionId || command.state !== 'queued') continue;
      const connection = this.connections.get(command.connection_id);
      if (connection) connection.queue = connection.queue.filter((id) => id !== command.command_id);
      this.finishCommand(command, null, new BrokerError(
        'CANCELLED', 'Agent disconnected before command dispatch', { retry_safe: true },
      ));
    }
    this.persistState();
    return { orphaned_tabs: orphaned };
  }

  connectExtension(payload) {
    const browserInstanceId = cleanId(payload.browser_instance_id, 'browser_instance_id');
    const browserFamily = payload.browser_family === undefined
      ? undefined : cleanBrowserFamily(payload.browser_family);
    const extensionCandidate = payload.extension_candidate === undefined
      ? undefined : cleanExtensionCandidate(payload.extension_candidate);
    const browserSessionId = typeof payload.browser_session_id === 'string'
      ? cleanId(payload.browser_session_id, 'browser_session_id') : undefined;
    const workerInstanceId = typeof payload.worker_instance_id === 'string'
      ? cleanId(payload.worker_instance_id, 'worker_instance_id') : undefined;
    const connectionId = opaque('extension');
    for (const connection of this.connections.values()) {
      if (connection.browser_instance_id === browserInstanceId && connection.state === 'online') {
        this.disconnectExtension(connection.connection_id, 'replaced_connection', {
          reconnectPending: true,
          replacementBrowserFamily: payload.browser_family,
          replacementBrowserSessionId: browserSessionId,
          replacementWorkerInstanceId: workerInstanceId,
        });
      }
    }
    this.connections.set(connectionId, {
      connection_id: connectionId,
      browser_instance_id: browserInstanceId,
      browser_family: browserFamily,
      browser_session_id: browserSessionId,
      worker_instance_id: workerInstanceId,
      extension_candidate: extensionCandidate,
      authorized_tabs: Array.isArray(payload.authorized_tabs) ? payload.authorized_tabs.slice(0, 256) : [],
      state: 'online',
      connected_at: this.now(),
      last_seen_at: this.now(),
      poll_count: 0,
      keepalive_count: 0,
      keepalive_socket: null,
      queue: [],
      waiters: [],
    });
    const connection = this.connections.get(connectionId);
    for (const command of this.commands.values()) {
      if (command.state === 'queued' && command.browser_instance_id === browserInstanceId) {
        connection.queue.push(command.command_id);
      }
    }
    this.record({ stage: 'extension', code: 'connected', browser_instance_id: browserInstanceId });
    return {
      schema: BROKER_SCHEMA,
      connection_id: connectionId,
      broker_epoch: this.epoch,
      require_full_truth: true,
      heartbeat_ms: 10_000,
    };
  }

  disconnectExtension(connectionId, reason = 'disconnected', {
    reconnectPending = false, replacementBrowserFamily,
    replacementBrowserSessionId, replacementWorkerInstanceId,
  } = {}) {
    const connection = this.connections.get(connectionId);
    if (!connection || connection.state !== 'online') return;
    connection.state = 'offline';
    connection.disconnected_at = this.now();
    if (connection.keepalive_socket) {
      const socket = connection.keepalive_socket;
      connection.keepalive_socket = null;
      try { socket.close(1000, 'connection replaced'); } catch (_error) { /* already closed */ }
    }
    for (const waiter of connection.waiters.splice(0)) waiter.finish([]);
    for (const command of this.commands.values()) {
      if (command.state === 'queued'
          && command.browser_instance_id === connection.browser_instance_id
          && !reconnectPending) {
        this.finishCommand(command, null, new BrokerError(
          'EXTENSION_OFFLINE',
          'Extension disconnected before command dispatch',
          { outcome: 'rejected', retry_safe: true },
        ));
      } else if (command.connection_id === connectionId
          && command.state === 'delivered'
          && command.idempotent
          && this.now() < command.deadline_at) {
        command.state = 'queued';
        command.connection_id = null;
      } else if (command.connection_id === connectionId && command.state === 'delivered') {
        this.finishCommand(command, null, new BrokerError(
          'OUTCOME_UNKNOWN',
          'Extension disconnected after command dispatch',
          { outcome: 'outcome_unknown', retry_safe: false },
        ));
      }
    }
    this.record({
      stage: 'extension', code: reason, connection_id: connectionId,
      browser_family: connection.browser_family,
      replacement_browser_family: replacementBrowserFamily,
      same_browser_session: replacementBrowserSessionId === undefined
        ? undefined : replacementBrowserSessionId === connection.browser_session_id,
      same_worker_instance: replacementWorkerInstanceId === undefined
        ? undefined : replacementWorkerInstanceId === connection.worker_instance_id,
      poll_count: connection.poll_count,
      connection_age_ms: this.now() - connection.connected_at,
      ms_since_last_poll: connection.poll_count
        ? this.now() - connection.last_seen_at : undefined,
    });
  }

  activeConnection(browserInstanceId) {
    return [...this.connections.values()]
      .filter((connection) => connection.state === 'online'
        && (!browserInstanceId || connection.browser_instance_id === browserInstanceId))
      .sort((left, right) => right.connected_at - left.connected_at)[0];
  }

  async pollCommands(connectionId, timeoutMs = EXTENSION_POLL_HEARTBEAT_MS) {
    const connection = this.connections.get(cleanId(connectionId, 'connection_id'));
    if (!connection || connection.state !== 'online') throw new BrokerError('EXTENSION_OFFLINE', 'Extension connection is offline');
    connection.last_seen_at = this.now();
    connection.poll_count += 1;
    const immediate = this.takeCommands(connection);
    if (immediate.length) return immediate;
    return new Promise((resolve) => {
      let completed = false;
      const waiter = { finish: null };
      const finish = (commands) => {
        if (completed) return;
        completed = true;
        clearTimeout(timer);
        const index = connection.waiters.indexOf(waiter);
        if (index !== -1) connection.waiters.splice(index, 1);
        resolve(commands);
      };
      waiter.finish = finish;
      const timer = setTimeout(() => finish([]), Math.min(timeoutMs, EXTENSION_POLL_HEARTBEAT_MS));
      connection.waiters.push(waiter);
    });
  }

  takeCommands(connection) {
    const ids = connection.queue.splice(0, COMMAND_LIMIT);
    const commands = [];
    for (const id of ids) {
      const command = this.commands.get(id);
      if (!command || command.state !== 'queued') continue;
      if (command.browser_instance_id !== connection.browser_instance_id) continue;
      if (this.now() >= command.deadline_at) {
        this.finishCommand(command, null, new BrokerError('DEADLINE_EXCEEDED', 'Command deadline exceeded'));
        continue;
      }
      command.state = 'delivered';
      command.delivered_at = this.now();
      command.connection_id = connection.connection_id;
      this.updateOccurrence(command, 'dispatched');
      commands.push({
        command_id: command.command_id,
        kind: command.kind,
        payload: command.payload,
        agent_session_id: command.agent_session_id,
        deadline_at: command.deadline_at,
      });
    }
    if (commands.length) this.persistState();
    return commands;
  }

  wakeConnection(connection) {
    if (!connection.waiters.length) return;
    const waiter = connection.waiters.shift();
    const commands = this.takeCommands(connection);
    waiter.finish(commands);
  }

  enqueueCommand(agentSessionId, kind, payload, timeoutMs, {
    idempotent = false, clientRequestId, browserInstanceId,
  } = {}) {
    this.touchSession(agentSessionId);
    const requestKey = `${agentSessionId}\u0000${JSON.stringify(clientRequestId)}`;
    if (clientRequestId !== undefined && this.cancelledRequests.delete(requestKey)) {
      throw new BrokerError('CANCELLED', 'Command cancelled before dispatch', { retry_safe: true });
    }
    if (this.commands.size >= COMMAND_LIMIT) {
      throw new BrokerError('BROKER_OVERLOADED', 'Broker command capacity is exhausted', { retry_safe: true });
    }
    const connection = this.activeConnection(browserInstanceId);
    if (!connection) throw new BrokerError('EXTENSION_OFFLINE', 'Extension is not connected', { retry_safe: true });
    const commandId = opaque('command');
    const deadlineAt = this.now() + boundedTimeout(timeoutMs);
    return new Promise((resolve, reject) => {
      const command = {
        command_id: commandId,
        client_request_id: clientRequestId,
        agent_session_id: agentSessionId,
        kind,
        payload,
        idempotent,
        state: 'queued',
        created_at: this.now(),
        deadline_at: deadlineAt,
        browser_instance_id: connection.browser_instance_id,
        connection_id: null,
        resolve,
        reject,
      };
      this.commands.set(commandId, command);
      connection.queue.push(commandId);
      const timer = setTimeout(() => {
        if (!['complete', 'failed'].includes(command.state)) {
          const delivered = command.state === 'delivered';
          this.finishCommand(command, null, new BrokerError(
            delivered ? 'OUTCOME_UNKNOWN' : 'DEADLINE_EXCEEDED',
            delivered ? 'Command outcome is unknown after deadline' : 'Command was not dispatched before deadline',
            { outcome: delivered ? 'outcome_unknown' : 'rejected', retry_safe: !delivered },
          ));
        }
      }, Math.max(1, deadlineAt - this.now()));
      command.timer = timer;
      this.wakeConnection(connection);
    });
  }

  cancelCommand(agentSessionId, commandId) {
    this.touchSession(agentSessionId);
    const command = this.commands.get(commandId);
    if (!command || command.agent_session_id !== agentSessionId) throw new BrokerError('COMMAND_UNKNOWN', 'Command is unknown');
    if (command.state === 'queued') {
      const connection = this.connections.get(command.connection_id);
      if (connection) connection.queue = connection.queue.filter((id) => id !== command.command_id);
      this.finishCommand(command, null, new BrokerError('CANCELLED', 'Command cancelled before dispatch', { retry_safe: true }));
      return { cancelled: true, dispatched: false };
    }
    return { cancelled: false, dispatched: true, reconciliation_required: true };
  }

  cancelRequest(agentSessionId, clientRequestId) {
    this.touchSession(agentSessionId);
    const command = [...this.commands.values()].find((candidate) => (
      candidate.agent_session_id === agentSessionId
      && candidate.client_request_id === clientRequestId
      && !['complete', 'failed'].includes(candidate.state)
    ));
    if (!command) {
      if (this.cancelledRequests.size >= COMMAND_LIMIT) {
        this.cancelledRequests.delete(this.cancelledRequests.values().next().value);
      }
      this.cancelledRequests.add(`${agentSessionId}\u0000${JSON.stringify(clientRequestId)}`);
      return { cancelled: true, dispatched: false, code: 'CANCELLED_BEFORE_QUEUE' };
    }
    return this.cancelCommand(agentSessionId, command.command_id);
  }

  finishCommand(command, result, error) {
    if (['complete', 'failed'].includes(command.state)) return;
    clearTimeout(command.timer);
    command.completed_at = this.now();
    this.updateOccurrence(
      command,
      error?.code === 'OUTCOME_UNKNOWN' ? 'outcome_unknown' : error ? 'rejected' : 'acknowledged',
      error,
    );
    try { this.persistState(); }
    catch (_persistError) {
      error = new BrokerError(
        'OUTCOME_UNKNOWN',
        'Command completed but its occurrence could not be durably recorded',
        { stage: 'broker_state', outcome: 'outcome_unknown', retry_safe: false },
      );
      this.updateOccurrence(command, 'outcome_unknown', error);
    }
    scrubCommandPayload(command);
    command.state = error ? 'failed' : 'complete';
    this.record({
      command_id: command.command_id,
      agent_session_id: command.agent_session_id,
      stage: error?.details?.stage || 'broker',
      code: error?.code || 'acknowledged',
      elapsed_ms: command.completed_at - command.created_at,
      message_bytes: result ? jsonSize(result) : 0,
      retry_safe: error?.retry_safe,
      current_revision: error?.current_revision,
    });
    if (error) command.reject(error);
    else command.resolve({ command_id: command.command_id, ...result });
    setTimeout(() => this.commands.delete(command.command_id), 60_000).unref?.();
  }

  acceptExtensionEvents(connectionId, events) {
    const connection = this.connections.get(cleanId(connectionId, 'connection_id'));
    if (!connection || connection.state !== 'online') throw new BrokerError('EXTENSION_OFFLINE', 'Extension connection is offline');
    connection.last_seen_at = this.now();
    if (!Array.isArray(events) || events.length > 256) throw new BrokerError('INVALID_MESSAGE', 'events must be a bounded array');
    for (const event of events) {
      if (event.kind === 'response') {
        const command = this.commands.get(event.command_id);
        if (!command || command.connection_id !== connectionId || command.state !== 'delivered') continue;
        if (event.error) this.finishCommand(command, null, new BrokerError(
          event.error.code || 'EXTENSION_REJECTED',
          event.error.message || 'Extension rejected command',
          event.error,
        ));
        else this.finishCommand(command, event.result || {}, null);
      } else if (event.kind === 'observation' || event.kind === 'observation.delta') {
        this.acceptTruth(event.kind, event.payload);
      } else if (event.kind !== 'heartbeat') {
        throw new BrokerError('INVALID_MESSAGE', `unsupported Extension event ${event.kind}`);
      }
    }
    return { accepted: events.length };
  }

  acceptTruth(kind, payload) {
    const tabId = cleanId(payload?.tab_id, 'tab_id');
    const documentId = cleanId(payload?.document_id, 'document_id');
    const revision = payload?.revision;
    if (!Number.isSafeInteger(revision) || revision < 1) throw new BrokerError('INVALID_TRUTH', 'Truth revision is invalid');
    const existing = this.truth.get(tabId);
    if (kind === 'observation') {
      this.truth.set(tabId, {
        tab_id: tabId,
        document_id: documentId,
        revision,
        full: payload,
        history: [],
      });
    } else {
      if (!existing || existing.document_id !== documentId
          || payload.base_revision !== existing.revision || revision !== existing.revision + 1) {
        this.truth.delete(tabId);
        this.emit(`truth:${tabId}`);
        return;
      }
      existing.full = materializeDelta(existing.full, payload);
      existing.revision = revision;
      existing.history.push(payload);
      if (existing.history.length > HISTORY_LIMIT) existing.history.shift();
    }
    this.emit(`truth:${tabId}`);
  }

  async waitForTruth(tabId, predicate, deadlineAt) {
    const current = this.truth.get(tabId);
    if (current && predicate(current)) return current;
    const remaining = deadlineAt - this.now();
    if (remaining <= 0) return null;
    return new Promise((resolve) => {
      let settled = false;
      const finish = (value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.off(`truth:${tabId}`, changed);
        resolve(value);
      };
      const changed = () => {
        const next = this.truth.get(tabId);
        if (next && predicate(next)) finish(next);
      };
      const timer = setTimeout(() => finish(null), remaining);
      this.on(`truth:${tabId}`, changed);
      changed();
    });
  }

  leaseTab(tabId, agentSessionId, metadata = {}) {
    this.touchSession(agentSessionId);
    const current = this.leases.get(tabId);
    if (current && (current.agent_session_id !== agentSessionId || current.state !== 'active')) {
      throw new BrokerError('TAB_ALREADY_LEASED', 'Tab already has a writer');
    }
    const replacement = {
      tab_id: tabId,
      agent_session_id: agentSessionId,
      state: 'active',
      leased_at: this.now(),
      ...metadata,
    };
    this.leases.set(tabId, replacement);
    try { this.persistState(); }
    catch (error) {
      if (current) this.leases.set(tabId, current);
      else this.leases.delete(tabId);
      throw error;
    }
  }

  requireLease(tabId, agentSessionId) {
    this.touchSession(agentSessionId);
    const lease = this.leases.get(cleanId(tabId, 'tab_id'));
    if (!lease) throw new BrokerError('TAB_NOT_LEASED', 'Tab is not leased to this Agent');
    if (lease.state === 'orphaned') throw new BrokerError('TAB_ORPHANED', 'Tab lease is orphaned');
    if (lease.agent_session_id !== agentSessionId) throw new BrokerError('TAB_LEASED_ELSEWHERE', 'Tab is leased to another Agent');
    return lease;
  }

  listTabs(agentSessionId) {
    this.touchSession(agentSessionId);
    return [...this.leases.values()]
      .filter((lease) => lease.agent_session_id === agentSessionId && lease.state === 'active')
      .map((lease) => {
        const truth = this.truth.get(lease.tab_id);
        const browserInstanceId = lease.browser_instance_id || truth?.full?.browser_instance_id;
        const connection = this.activeConnection(browserInstanceId);
        return {
          tab_id: lease.tab_id,
          browser_instance_id: browserInstanceId,
          browser_family: connection?.browser_family,
          extension_candidate: connection?.extension_candidate,
          ownership: lease.ownership || 'agent',
          document_id: truth?.document_id,
          revision: truth?.revision,
          readiness: truth ? 'truth_ready' : 'awaiting_truth',
        };
      });
  }

  readTruthNow(agentSessionId, params) {
    const tabId = cleanId(params.tab_id, 'tab_id');
    this.requireLease(tabId, agentSessionId);
    const mode = params.mode;
    if (!['full', 'delta'].includes(mode)) throw new BrokerError('INVALID_REQUEST', 'mode must be full or delta');
    const current = this.truth.get(tabId);
    if (!current) throw new BrokerError('TRUTH_UNAVAILABLE', 'Canonical Truth is not available for this tab', { retry_safe: true });
    if (mode === 'full') return projectTruth(current.full, params.query, {
      mode: 'full', complete: true, next_basis_revision: current.revision,
    });
    const after = params.after_revision;
    if (!Number.isSafeInteger(after) || after < 0) throw new BrokerError('INVALID_REQUEST', 'delta mode requires after_revision');
    if (after === current.revision) return {
      schema: 'saccade.agent-truth/2', mode: 'delta', tab_id: tabId,
      document_id: current.document_id, revision: current.revision,
      complete: true, changes: [], next_basis_revision: current.revision,
    };
    const deltas = current.history.filter((delta) => delta.revision > after);
    if (!deltas.length || deltas[0].base_revision !== after
        || deltas.at(-1).revision !== current.revision) {
      return {
        schema: 'saccade.agent-truth/2', mode: 'delta', tab_id: tabId,
        document_id: current.document_id, revision: current.revision,
        complete: false, reset_required: true, next_basis_revision: current.revision,
      };
    }
    const changes = deltas.flatMap((delta) => delta.changes || []);
    const changedIds = new Set(changes.filter((change) => change.kind !== 'disappeared').map((change) => change.object_id));
    const objects = (current.full.objects || []).filter((object) => changedIds.has(object.object_id));
    return projectTruth({
      schema: 'saccade.agent-truth/2', tab_id: tabId,
      document_id: current.document_id, revision: current.revision, changes, objects,
    }, params.query, {
      mode: 'delta', complete: true, next_basis_revision: current.revision,
    });
  }

  async readTruth(agentSessionId, params, deadlineAt) {
    const tabId = cleanId(params.tab_id, 'tab_id');
    this.requireLease(tabId, agentSessionId);
    const minObjects = params.min_objects;
    if (minObjects !== undefined && (!Number.isSafeInteger(minObjects) || minObjects < 1 || minObjects > 32)) {
      throw new BrokerError('INVALID_REQUEST', 'min_objects must be an integer from 1 to 32');
    }
    if (minObjects !== undefined && !params.query) {
      throw new BrokerError('INVALID_REQUEST', 'min_objects requires a semantic query');
    }
    const queryReady = (truth) => minObjects === undefined
      || matchingObjects(truth.full?.objects || [], params.query).length >= minObjects;
    if (!this.truth.get(tabId)) {
      const available = await this.waitForTruth(tabId, queryReady, deadlineAt);
      if (!available) throw new BrokerError(
        'TRUTH_TIMEOUT', 'Canonical Truth did not arrive before the request deadline',
        { stage: 'broker_truth_wait', retry_safe: true },
      );
    }
    if (params.mode === 'delta' && Number.isSafeInteger(params.after_revision)) {
      const current = this.truth.get(tabId);
      if (current && current.revision === params.after_revision) {
        const changed = await this.waitForTruth(tabId, (truth) => (
          (truth.document_id !== current.document_id || truth.revision > params.after_revision)
          && queryReady(truth)
        ), deadlineAt);
        if (!changed) return {
          schema: 'saccade.agent-truth/2', mode: 'delta', tab_id: tabId,
          document_id: current.document_id, revision: current.revision,
          complete: true, changes: [], timed_out: true,
          next_basis_revision: current.revision,
        };
      }
    }
    if (!queryReady(this.truth.get(tabId))) {
      const matched = await this.waitForTruth(tabId, queryReady, deadlineAt);
      if (!matched) throw new BrokerError(
        'TRUTH_TIMEOUT', 'Semantic Truth condition did not arrive before the request deadline',
        { stage: 'broker_truth_wait', retry_safe: true, current_revision: this.truth.get(tabId)?.revision },
      );
    }
    return this.readTruthNow(agentSessionId, params);
  }

  async runReflex(agentSessionId, params, deadlineAt, clientRequestId) {
    this.requireLease(params.tab_id, agentSessionId);
    if (params.steps || (params.operation && params.operation !== 'click')) {
      throw new BrokerError('INVALID_REQUEST', 'bounded reflex execution requires one click object');
    }
    if (!Number.isSafeInteger(params.max_actions) || params.max_actions < 1 || params.max_actions > 1000) {
      throw new BrokerError('INVALID_REQUEST', 'max_actions must be an integer from 1 to 1000');
    }
    const launchOnly = params.object_id === undefined && params.start_object_id !== undefined;
    if (params.object_id === undefined && !launchOnly) {
      throw new BrokerError('INVALID_REQUEST', 'bounded reflex execution requires a current controller or explicit start_object_id');
    }
    const initial = this.truth.get(params.tab_id);
    // A reflex controller carries no action token. Its document-local object
    // identity and loop-class authority can remain current while moving
    // targets advance unrelated geometry revisions every frame. Rebase only
    // this dedicated controller form onto the current canonical Truth; a
    // missing/replaced controller, document change, or future basis is stale.
    if (!initial || initial.document_id !== params.document_id
      || !Number.isSafeInteger(params.basis_revision)
      || params.basis_revision > initial.revision
      || (launchOnly && params.basis_revision !== initial.revision)) {
      throw new BrokerError('STALE_AUTHORITY', 'Action document or revision is stale', {
        retry_safe: true, current_revision: initial?.revision,
      });
    }
    let controller;
    if (!launchOnly) {
      const controllers = (initial.full.objects || []).filter((object) => object.object_id === params.object_id);
      if (controllers.length !== 1) {
        throw new BrokerError(controllers.length ? 'AMBIGUOUS_OBJECT' : 'OBJECT_UNKNOWN', 'object_id must resolve exactly once');
      }
      [controller] = controllers;
      if (controller.role !== 'reflex_target' || typeof controller.loop_class_token !== 'string') {
        throw new BrokerError('AFFORDANCE_MISMATCH', 'max_actions is limited to a current reflex loop authority');
      }
    }

    let startTarget;
    if (params.start_object_id !== undefined) {
      const starts = (initial.full.objects || []).filter((object) => object.object_id === params.start_object_id);
      if (starts.length !== 1) {
        throw new BrokerError(starts.length ? 'AMBIGUOUS_OBJECT' : 'OBJECT_UNKNOWN', 'start_object_id must resolve exactly once');
      }
      startTarget = starts[0];
      const rootFrame = (initial.full.frames || []).find((frame) => (
        frame.document_id === initial.document_id && !frame.parent_frame_id
      ));
      let sameOriginNavigation = false;
      try {
        sameOriginNavigation = startTarget.role === 'link'
          && !startTarget.navigation_disposition
          && new URL(startTarget.navigation_target).origin === new URL(rootFrame?.document_url).origin;
      } catch (_error) {
        sameOriginNavigation = false;
      }
      if (!startTarget.action_token || !(startTarget.affordances || []).includes('click')
        || (startTarget.role !== 'button' && !sameOriginNavigation)
        || (launchOnly && !sameOriginNavigation)) {
        throw new BrokerError(
          'AFFORDANCE_MISMATCH',
          'start_object_id must be one current clickable button or same-origin navigation link',
        );
      }
    }

    const startedAt = this.now();
    let documentId = initial.document_id;
    let loopClass = controller?.loop_class_token;
    let cursorRevision = initial.revision;
    let attemptedToken;
    let actions = 0;
    let staleRetries = 0;
    let stopReason = 'deadline';
    const receipts = [];
    const occurrenceFor = (truth) => (truth?.full?.objects || []).find((object) => (
      object.role === 'reflex_target' && object.loop_class_token === loopClass
    ))?.state?.reflex_occurrence;

    if (startTarget) {
      const remainingMs = Math.max(1, deadlineAt - this.now());
      const startReceipt = await this.rpc(agentSessionId, 'act', {
        tab_id: params.tab_id,
        document_id: initial.document_id,
        basis_revision: initial.revision,
        object_id: startTarget.object_id,
        operation: 'click',
        timeout_ms: remainingMs,
      }, remainingMs, clientRequestId);
      if (startReceipt.outcome !== 'accepted' || startReceipt.semantic_postcondition?.verified !== true) {
        return {
          schema: 'saccade.reflex-report/1',
          outcome: startReceipt.outcome,
          occurrence: startReceipt.occurrence,
          semantic_postcondition: { code: 'start_not_verified', verified: false },
          document_id: startReceipt.document_id || documentId,
          final_revision: startReceipt.final_revision || cursorRevision,
          next_basis_revision: startReceipt.next_basis_revision || cursorRevision,
          actions: 0, stale_retries: 0, duration_ms: this.now() - startedAt,
          stop_reason: 'start_not_verified', receipts: [], retry_safe: false,
          external_execution_required: false,
        };
      }
      cursorRevision = startReceipt.next_basis_revision;
      if (launchOnly) {
        const launched = await this.waitForTruth(params.tab_id, (truth) => {
          if (truth.document_id === params.document_id && truth.revision <= params.basis_revision) return false;
          const controllers = (truth.full.objects || []).filter((object) => (
            object.role === 'reflex_target'
            && typeof object.loop_class_token === 'string'
            && !object.action_token
          ));
          return controllers.length === 1;
        }, deadlineAt);
        if (!launched) {
          const current = this.truth.get(params.tab_id);
          return {
            schema: 'saccade.reflex-report/1', outcome: 'accepted', occurrence: 'not_observed',
            semantic_postcondition: { code: 'start_controller_unavailable', verified: false },
            document_id: current?.document_id || startReceipt.document_id,
            final_revision: current?.revision || startReceipt.final_revision,
            next_basis_revision: current?.revision || startReceipt.next_basis_revision,
            actions: 0, stale_retries: 0, duration_ms: this.now() - startedAt,
            stop_reason: 'start_controller_unavailable', receipts: [], retry_safe: false,
            external_execution_required: false,
          };
        }
        const controllers = (launched.full.objects || []).filter((object) => (
          object.role === 'reflex_target'
          && typeof object.loop_class_token === 'string'
          && !object.action_token
        ));
        [controller] = controllers;
        documentId = launched.document_id;
        loopClass = controller.loop_class_token;
        cursorRevision = launched.revision;
      }
    }

    while (this.now() < deadlineAt && actions < params.max_actions) {
      const current = this.truth.get(params.tab_id);
      if (!current || current.document_id !== documentId) {
        stopReason = 'document_changed';
        break;
      }
      cursorRevision = Math.max(cursorRevision, current.revision);
      const target = [...(current.full.objects || [])].reverse().find((object) => (
        object.role === 'reflex_target'
        && object.loop_class_token === loopClass
        && object.action_token
        && (object.affordances || []).includes('click')
        && object.state?.enabled === 'true'
        && object.action_token !== attemptedToken
      ));
      if (!target) {
        const next = await this.waitForTruth(params.tab_id, (truth) => (
          truth.document_id !== documentId || truth.revision > cursorRevision
        ), Math.min(deadlineAt, this.now() + 50));
        if (next) cursorRevision = next.revision;
        continue;
      }

      attemptedToken = target.action_token;
      const beforeOccurrence = target.state?.reflex_occurrence ?? occurrenceFor(current);
      try {
        const remainingMs = Math.max(1, deadlineAt - this.now());
        const receipt = await this.rpc(agentSessionId, 'act', {
          tab_id: params.tab_id,
          document_id: current.document_id,
          basis_revision: current.revision,
          object_id: target.object_id,
          operation: 'click',
          timeout_ms: remainingMs,
        }, remainingMs, clientRequestId);
        if (receipt.outcome !== 'accepted' || receipt.semantic_postcondition?.verified !== true) {
          stopReason = receipt.outcome === 'outcome_unknown' ? 'outcome_unknown' : 'unverified';
          receipts.push({ object_id: target.object_id, outcome: receipt.outcome, before_occurrence: beforeOccurrence });
          break;
        }
        const verifiedTruth = await this.waitForTruth(params.tab_id, (truth) => (
          truth.document_id !== documentId || occurrenceFor(truth) !== beforeOccurrence
        ), Math.min(deadlineAt, this.now() + 250));
        if (!verifiedTruth || verifiedTruth.document_id !== documentId) {
          stopReason = 'occurrence_unverified';
          receipts.push({ object_id: target.object_id, outcome: 'outcome_unknown', before_occurrence: beforeOccurrence });
          break;
        }
        actions += 1;
        cursorRevision = verifiedTruth.revision;
        receipts.push({
          object_id: target.object_id,
          outcome: 'accepted',
          before_occurrence: beforeOccurrence,
          after_occurrence: occurrenceFor(verifiedTruth),
          final_revision: verifiedTruth.revision,
        });
      } catch (error) {
        if (['STALE_AUTHORITY', 'OBJECT_UNKNOWN', 'ACTION_UNAVAILABLE', 'AFFORDANCE_MISMATCH'].includes(error.code)) {
          staleRetries += 1;
          continue;
        }
        throw error;
      }
    }
    if (actions >= params.max_actions) stopReason = 'max_actions';
    const finalTruth = this.truth.get(params.tab_id);
    return {
      schema: 'saccade.reflex-report/1',
      outcome: stopReason === 'outcome_unknown' || stopReason === 'occurrence_unverified' ? 'outcome_unknown' : 'accepted',
      occurrence: actions ? 'observed' : 'not_observed',
      semantic_postcondition: {
        code: actions ? 'reflex_occurrences_verified' : stopReason,
        verified: actions > 0 && !['outcome_unknown', 'occurrence_unverified'].includes(stopReason),
      },
      document_id: finalTruth?.document_id || documentId,
      final_revision: finalTruth?.revision || cursorRevision,
      next_basis_revision: finalTruth?.revision || cursorRevision,
      actions,
      stale_retries: staleRetries,
      duration_ms: this.now() - startedAt,
      stop_reason: stopReason,
      receipts: receipts.slice(-16),
      retry_safe: false,
      external_execution_required: false,
    };
  }

  async rpc(agentSessionId, method, params = {}, timeoutMs = 10_000, clientRequestId) {
    this.touchSession(agentSessionId);
    const deadlineAt = this.now() + boundedTimeout(timeoutMs);
    const remaining = () => Math.max(1, deadlineAt - this.now());
    if (method === 'system.capabilities') {
      const tabs = this.listTabs(agentSessionId);
      const connectedExtensions = [...this.connections.values()]
        .filter((connection) => connection.state === 'online')
        .sort((left, right) => left.connected_at - right.connected_at)
        .map((connection) => ({
          browser_instance_id: connection.browser_instance_id,
          browser_family: connection.browser_family,
          extension_candidate: connection.extension_candidate,
        }));
      const attached = connectedExtensions.length === 1 ? connectedExtensions[0] : undefined;
      return {
        schema: 'saccade.capabilities/8', runtime: 'node', broker_schema: BROKER_SCHEMA,
        broker_epoch: this.epoch, agent_session_id: agentSessionId,
        extension_connected: connectedExtensions.length > 0,
        browser_family: attached?.browser_family,
        extension_candidate: attached?.extension_candidate,
        connected_extensions: connectedExtensions,
        browser_support: ['chrome', 'edge'], native_host: false, rust: false,
        truth_modes: ['full', 'delta'], exact_tab_routing: true,
        leased_tabs: tabs, current_tab_id: tabs.length === 1 ? tabs[0].tab_id : null,
      };
    }
    if (method === 'tabs.list') return { tabs: this.listTabs(agentSessionId) };
    if (method === 'truth.read') return this.readTruth(agentSessionId, params, deadlineAt);
    if (method === 'tabs.open') {
      const hasUrl = typeof params.url === 'string' && params.url.length > 0;
      const hasClaim = typeof params.claim === 'string' && typeof params.tab_id === 'string';
      if (hasUrl === hasClaim) {
        throw new BrokerError('INVALID_REQUEST', 'tabs.open requires either url or claim with tab_id');
      }
      if (params.tab_id && params.claim !== 'arm') {
        const existingLease = this.leases.get(cleanId(params.tab_id, 'tab_id'));
        if (existingLease && (existingLease.agent_session_id !== agentSessionId || existingLease.state !== 'active')) {
          throw new BrokerError('TAB_ALREADY_LEASED', 'Tab already has a writer');
        }
      }
      const requestedBrowserInstanceId = params.browser_instance_id === undefined
        ? undefined : cleanId(params.browser_instance_id, 'browser_instance_id');
      const onlineConnections = [...this.connections.values()]
        .filter((connection) => connection.state === 'online');
      if (!requestedBrowserInstanceId && onlineConnections.length > 1) {
        throw new BrokerError('AMBIGUOUS_BROWSER', 'tabs.open requires browser_instance_id when multiple browsers are connected', {
          retry_safe: true,
          candidates: onlineConnections.slice(0, 8).map((connection) => ({
            browser_instance_id: connection.browser_instance_id,
            browser_family: connection.browser_family,
            extension_candidate: connection.extension_candidate,
          })),
        });
      }
      const browserInstanceId = requestedBrowserInstanceId
        || onlineConnections[0]?.browser_instance_id;
      const result = await this.enqueueCommand(agentSessionId, 'tabs.open', params, remaining(), {
        clientRequestId, browserInstanceId,
      });
      if (params.claim === 'arm') {
        return { ...result, agent_session_id: agentSessionId, lease: 'pending_claim' };
      }
      const tabId = cleanId(result.tab_id, 'tab_id');
      try {
        this.leaseTab(tabId, agentSessionId, {
          ownership: params.claim === 'shared' ? 'user_shared' : 'agent',
          browser_instance_id: result.browser_instance_id,
        });
      } catch (error) {
        if (error.code !== 'STATE_PERSIST_FAILED') throw error;
        throw new BrokerError(
          'OUTCOME_UNKNOWN', 'Tab opened but its lease could not be durably recorded',
          { stage: 'broker_state', outcome: 'outcome_unknown', retry_safe: false },
        );
      }
      const truth = await this.waitForTruth(tabId, () => true, deadlineAt);
      return {
        ...result, agent_session_id: agentSessionId, lease: 'active',
        document_id: truth?.document_id,
        initial_revision: truth?.revision,
        readiness: truth ? 'truth_ready' : 'awaiting_truth',
      };
    }
    if (method === 'tabs.close') {
      const lease = this.requireLease(params.tab_id, agentSessionId);
      const result = await this.enqueueCommand(agentSessionId, 'tabs.close', params, remaining(), {
        clientRequestId, browserInstanceId: lease.browser_instance_id || this.truth.get(params.tab_id)?.full?.browser_instance_id,
      });
      this.leases.delete(params.tab_id);
      this.truth.delete(params.tab_id);
      try { this.persistState(); }
      catch (error) {
        if (error.code !== 'STATE_PERSIST_FAILED') throw error;
        throw new BrokerError(
          'OUTCOME_UNKNOWN', 'Tab closed but its lease removal could not be durably recorded',
          { stage: 'broker_state', outcome: 'outcome_unknown', retry_safe: false },
        );
      }
      return result;
    }
    if (method === 'act') {
      if (params.max_actions !== undefined) {
        return this.runReflex(agentSessionId, params, deadlineAt, clientRequestId);
      }
      this.requireLease(params.tab_id, agentSessionId);
      if (Boolean(params.steps) === Boolean(params.object_id)) {
        throw new BrokerError('INVALID_REQUEST', 'act requires exactly one of object_id or steps');
      }
      const current = this.truth.get(params.tab_id);
      const inputSteps = params.steps || [params];
      if (!Array.isArray(inputSteps) || inputSteps.length < 1 || inputSteps.length > 32) {
        throw new BrokerError('INVALID_BATCH', 'steps must contain 1 to 32 independent form actions');
      }
      if (!current || current.document_id !== params.document_id) {
        throw new BrokerError('STALE_AUTHORITY', 'Action document or revision is stale', { retry_safe: true, current_revision: current?.revision });
      }
      const requestedBasisRevision = params.basis_revision;
      const basisRevision = rebaseOrdinaryActionBasis(current, requestedBasisRevision, inputSteps);
      const basisDocumentId = current.document_id;
      const seen = new Set();
      const steps = inputSteps.map((step) => {
        const matches = (current.full.objects || []).filter((object) => object.object_id === step.object_id);
        if (matches.length !== 1) throw new BrokerError(matches.length ? 'AMBIGUOUS_OBJECT' : 'OBJECT_UNKNOWN', 'object_id must resolve exactly once');
        const target = matches[0];
        if (seen.has(target.object_id)) throw new BrokerError('INVALID_BATCH', 'batch steps must address independent objects');
        seen.add(target.object_id);
        if (!target.action_token) throw new BrokerError('ACTION_UNAVAILABLE', 'Object has no current action authority');
        const operation = step.operation || inferOperation(step, target);
        if (!(target.affordances || []).includes(operation)) {
          throw new BrokerError('AFFORDANCE_MISMATCH', 'operation is not a current affordance', { retry_safe: true });
        }
        if (params.steps && operation === 'click' && !['checkbox', 'radio', 'switch'].includes(target.role)) {
          throw new BrokerError('BATCH_BOUNDARY', 'batch clicks are limited to non-submitting form toggles');
        }
        if (params.steps && !['click', 'type', 'select'].includes(operation)) {
          throw new BrokerError('BATCH_BOUNDARY', 'submit, navigation, and upload are not allowed in a form batch');
        }
        if (operation !== 'upload' && (step.file_path !== undefined || step.file_sha256 !== undefined)) {
          throw new BrokerError('INVALID_REQUEST', 'file_path is valid only for an upload action', { retry_safe: true });
        }
        const payload = operation === 'type'
          ? { kind: 'text', text: String(step.text ?? step.value ?? '') }
          : operation === 'select'
            ? { kind: 'select', option_object_id: step.option_object_id }
            : operation === 'upload'
              ? materializeUploadFile(
                step.file_path, step.file_sha256,
                this.sessions.get(agentSessionId)?.upload_roots || [],
              )
              : { kind: 'none' };
        return {
          browser_instance_id: current.full.browser_instance_id,
          tab_id: params.tab_id, document_id: params.document_id,
          basis_revision: basisRevision, object_id: target.object_id,
          action_token: target.action_token, operation, payload,
          timeout_ms: Math.min(boundedTimeout(params.timeout_ms, 5_000), remaining()),
        };
      });
      const batch = Boolean(params.steps);
      const command = batch ? {
        tab_id: params.tab_id, document_id: params.document_id,
        basis_revision: basisRevision, timeout_ms: Math.min(boundedTimeout(params.timeout_ms, 5_000), remaining()),
        steps,
      } : steps[0];
      const result = await this.enqueueCommand(agentSessionId, batch ? 'act.batch' : 'act', command, remaining(), {
        clientRequestId, browserInstanceId: current.full.browser_instance_id,
      });
      const partialDispatch = result.partial_dispatch === true;
      const dispatchDocumentId = typeof result.dispatch_document_id === 'string'
        ? result.dispatch_document_id : basisDocumentId;
      const dispatchBasisRevision = Number.isSafeInteger(result.dispatch_basis_revision)
        && result.dispatch_basis_revision >= basisRevision
        ? result.dispatch_basis_revision : basisRevision;
      const extensionSemanticVerified = batch
        ? result.steps?.length === steps.length
          && result.steps.every((step) => step.accepted === true
            && step.semantic_postcondition?.verified === true)
        : result.semantic_postcondition?.verified === true;
      const finalTruth = result.accepted || partialDispatch
        ? extensionSemanticVerified
          ? this.truth.get(params.tab_id)?.full
          : await this.waitForTruth(params.tab_id, (truth) => (
            truth.document_id !== dispatchDocumentId || truth.revision > dispatchBasisRevision
          ), deadlineAt)
        : null;
      const truthAdvanced = Boolean(finalTruth && (
        finalTruth.document_id !== dispatchDocumentId || finalTruth.revision > dispatchBasisRevision
      ));
      const upload = !batch && steps[0].operation === 'upload' ? {
        size_bytes: steps[0].payload.file.size_bytes,
        mime_type: steps[0].payload.file.mime_type,
        sha256: steps[0].payload.file.sha256,
      } : undefined;
      const transition = truthAdvanced && finalTruth.document_id === dispatchDocumentId
        ? projectTruthToObjectIds(this.readTruthNow(agentSessionId, {
          tab_id: params.tab_id, mode: 'delta', after_revision: dispatchBasisRevision,
        }), new Set(steps.flatMap((step) => [step.object_id, step.payload?.option_object_id].filter(Boolean))))
        : undefined;
      const batchTransition = batch && transition
        ? compactBatchTransition(transition, steps, dispatchBasisRevision)
        : undefined;
      const verifiedStepIds = new Set(transition?.changes?.map((change) => change.object_id) || []);
      const stepReceipts = batch ? steps.map((step, index) => ({
        step_index: index,
        operation: step.operation,
        accepted: result.steps?.[index]?.accepted === true,
        verified: result.steps?.[index]?.semantic_postcondition?.verified === true
          || verifiedStepIds.has(step.object_id)
          || verifiedStepIds.has(step.payload?.option_object_id),
      })) : undefined;
      const semanticVerified = partialDispatch ? false : batch
        ? stepReceipts.every((step) => step.accepted && step.verified)
        : extensionSemanticVerified || truthAdvanced;
      return {
        command_id: result.command_id,
        outcome: partialDispatch ? 'outcome_unknown' : !result.accepted ? 'rejected' : semanticVerified ? 'accepted' : 'outcome_unknown',
        occurrence: partialDispatch ? 'partially_dispatched' : result.accepted ? (semanticVerified ? 'observed' : 'dispatched') : 'not_dispatched',
        semantic_postcondition: {
          code: partialDispatch ? (result.failure_code || 'partial_batch')
            : !result.accepted ? (result.failure_code || 'rejected')
              : semanticVerified ? (upload
                ? result.semantic_postcondition?.code
                  || (result.upload_dispatch === 'drop' ? 'file_drop_dispatched' : 'file_selection_observed')
                : result.semantic_postcondition?.code || 'truth_transition_observed')
                : batch && (truthAdvanced || extensionSemanticVerified)
                  ? 'batch_verification_incomplete' : 'verification_timeout',
          stage: !result.accepted ? result.failure_stage : undefined,
          verified: semanticVerified,
        },
        document_id: finalTruth?.document_id || basisDocumentId,
        dispatch_basis_revision: dispatchBasisRevision,
        final_revision: finalTruth?.revision || basisRevision,
        next_basis_revision: finalTruth?.revision || basisRevision,
        relevant_delta: batch ? batchTransition : transition,
        steps: stepReceipts,
        upload,
        rebased_from_revision: requestedBasisRevision < basisRevision ? requestedBasisRevision : undefined,
        retry_safe: partialDispatch ? false : !result.accepted ? result.retry_safe === true : false,
        external_execution_required: Boolean(upload),
      };
    }
    throw new BrokerError('METHOD_UNKNOWN', `Unknown Broker method ${method}`);
  }

  record(event) {
    this.diagnostics.push({ at: this.now(), ...event });
    if (this.diagnostics.length > DIAGNOSTIC_LIMIT) this.diagnostics.shift();
  }

  updateOccurrence(command, occurrence, error) {
    const existing = this.occurrences.find((value) => value.command_id === command.command_id);
    const value = {
      command_id: command.command_id,
      agent_session_id: command.agent_session_id,
      tab_id: typeof command.payload?.tab_id === 'string' ? command.payload.tab_id : undefined,
      kind: command.kind,
      created_at: command.created_at,
      delivered_at: command.delivered_at,
      completed_at: command.completed_at,
      occurrence,
      code: error?.code || (occurrence === 'acknowledged' ? 'ACKNOWLEDGED' : undefined),
      retry_safe: error?.retry_safe,
    };
    if (existing) Object.assign(existing, value);
    else this.occurrences.push(value);
    if (this.occurrences.length > OCCURRENCE_LIMIT) this.occurrences.shift();
  }

  doctor() {
    const failures = this.diagnostics.filter((event) => event.code !== 'acknowledged' && event.code !== 'connected').slice(-16);
    const onlineExtensions = [...this.connections.values()].filter((connection) => connection.state === 'online');
    return {
      schema: 'saccade.doctor/2', runtime: 'node', broker_epoch: this.epoch,
      extension_connected: Boolean(this.activeConnection()),
      online_extension_connections: onlineExtensions.length,
      extension_polls: onlineExtensions.reduce((total, connection) => total + connection.poll_count, 0),
      extension_poll_waiters: onlineExtensions.reduce((total, connection) => total + connection.waiters.length, 0),
      extension_keepalives: onlineExtensions.reduce((total, connection) => total + connection.keepalive_count, 0),
      extension_keepalive_connections: onlineExtensions.filter((connection) => connection.keepalive_socket).length,
      online_sessions: [...this.sessions.values()].filter((session) => session.state === 'online').length,
      active_leases: [...this.leases.values()].filter((lease) => lease.state === 'active').length,
      orphaned_leases: [...this.leases.values()].filter((lease) => lease.state === 'orphaned').length,
      queued_commands: [...this.commands.values()].filter((command) => command.state === 'queued').length,
      delivered_commands: [...this.commands.values()].filter((command) => command.state === 'delivered').length,
      recoverable_sessions: [...this.sessions.values()].filter((session) => session.state === 'recoverable').length,
      recoverable_leases: [...this.leases.values()].filter((lease) => lease.state === 'recoverable').length,
      outcome_unknown_occurrences: this.occurrences.filter((value) => value.occurrence === 'outcome_unknown').length,
      recent_failures: failures,
    };
  }
}

function inferOperation(params, target) {
  if (params.option_object_id) return 'select';
  if (params.text !== undefined || params.value !== undefined) return 'type';
  const affordances = target.affordances || [];
  if (affordances.length !== 1) throw new BrokerError('AMBIGUOUS_OPERATION', 'operation is ambiguous');
  return affordances[0];
}

function rebaseOrdinaryActionBasis(current, requestedBasisRevision, inputSteps) {
  const stale = () => {
    throw new BrokerError('STALE_AUTHORITY', 'Action document or revision is stale', {
      retry_safe: true, current_revision: current?.revision,
    });
  };
  if (!Number.isSafeInteger(requestedBasisRevision) || requestedBasisRevision < 1
      || requestedBasisRevision > current.revision) stale();
  if (requestedBasisRevision === current.revision) return current.revision;

  const protectedIds = new Set();
  for (const step of inputSteps) {
    if (typeof step?.object_id !== 'string') stale();
    protectedIds.add(step.object_id);
    if (step.option_object_id !== undefined) {
      if (typeof step.option_object_id !== 'string') stale();
      protectedIds.add(step.option_object_id);
    }
  }
  const deltas = (current.history || []).filter((delta) => delta.revision > requestedBasisRevision);
  if (!deltas.length || deltas[0].base_revision !== requestedBasisRevision
      || deltas.at(-1).revision !== current.revision) stale();
  let expectedBase = requestedBasisRevision;
  for (const delta of deltas) {
    if (delta.document_id !== current.document_id || delta.base_revision !== expectedBase
        || delta.revision !== expectedBase + 1) stale();
    if ((delta.changes || []).some((change) => protectedIds.has(change.object_id))) stale();
    expectedBase = delta.revision;
  }
  for (const objectId of protectedIds) {
    const matches = (current.full.objects || []).filter((object) => object.object_id === objectId);
    if (matches.length !== 1) stale();
  }
  return current.revision;
}

function materializeDelta(full, delta) {
  const objects = new Map((full.objects || []).map((object) => [object.object_id, object]));
  for (const change of delta.changes || []) {
    if (change.kind === 'disappeared') objects.delete(change.object_id);
  }
  for (const object of delta.objects || []) objects.set(object.object_id, object);
  for (const authority of delta.authorities || []) {
    const object = objects.get(authority.object_id);
    if (object) object.action_token = authority.action_token;
  }
  return { ...full, ...delta, objects: [...objects.values()], changes: delta.changes || [] };
}

function matchingObjects(objects, query) {
  const roles = new Set(query?.roles || []);
  const affordances = new Set(query?.affordances || []);
  const visibility = new Set(query?.visibility || []);
  const objectIds = new Set(query?.object_ids || []);
  const terms = String(query?.text || '').toLowerCase().split(/\s+/).filter(Boolean);
  return objects.filter((object) => {
    if (objectIds.size && !objectIds.has(object.object_id)) return false;
    if (roles.size && !roles.has(object.role)) return false;
    if (affordances.size && ![...affordances].every((item) => (object.affordances || []).includes(item))) return false;
    if (visibility.size && !visibility.has(object.visibility)) return false;
    const haystack = [object.name, object.text, object.description].filter(Boolean).join(' ').toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}

function retainRelatedCollections(projected, objectIds) {
  if (Array.isArray(projected.authorities)) {
    projected.authorities = projected.authorities.filter((authority) => objectIds.has(authority.object_id));
  }
  if (Array.isArray(projected.changes)) {
    projected.changes = projected.changes.filter((change) => objectIds.has(change.object_id));
  }
  return projected;
}

function projectTruthToObjectIds(value, objectIds) {
  const projected = structuredClone(value);
  if (Array.isArray(projected.objects)) {
    projected.objects = projected.objects.filter((object) => objectIds.has(object.object_id));
  }
  return retainRelatedCollections(projected, objectIds);
}

function compactBatchTransition(value, steps, baseRevision) {
  const changedIds = new Set((value.changes || []).map((change) => change.object_id));
  const changedSteps = [];
  for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    if (changedIds.has(step.object_id) || changedIds.has(step.payload?.option_object_id)) {
      changedSteps.push(index);
    }
  }
  return {
    schema: 'saccade.action-delta/1',
    base_revision: baseRevision,
    revision: value.revision,
    next_basis_revision: value.next_basis_revision,
    changed_steps: changedSteps,
  };
}

function projectTruth(value, query, envelope) {
  const projected = structuredClone(value);
  if (query && Array.isArray(projected.objects)) {
    const limit = Math.min(Number(query.max_objects) || 32, 32);
    const matches = matchingObjects(projected.objects, query);
    projected.objects = matches.slice(0, limit);
    retainRelatedCollections(projected, new Set(projected.objects.map((object) => object.object_id)));
    projected.match_count = matches.length;
    projected.working_set = 'semantic';
  } else if (envelope.mode === 'full' && Array.isArray(projected.objects) && projected.objects.length > 64) {
    projected.object_count = projected.objects.length;
    projected.catalog = 'complete_compact';
    projected.objects = projected.objects.map((object) => ({
      object_id: object.object_id,
      object_revision: object.object_revision,
      role: object.role,
      kind: object.kind,
      name: object.name,
      state: object.state,
      affordances: object.affordances,
      visibility: object.visibility,
      protected: object.protected,
      action_token: object.action_token,
    }));
  }
  return { ...projected, ...envelope, schema: 'saccade.agent-truth/2' };
}

function readBody(request) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let size = 0;
    request.on('data', (chunk) => {
      size += chunk.length;
      if (size > MAX_BODY_BYTES) {
        reject(new BrokerError('MESSAGE_TOO_LARGE', 'Message exceeds Broker limit'));
        request.destroy();
      } else chunks.push(chunk);
    });
    request.on('end', () => {
      try { resolve(chunks.length ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : {}); }
      catch (_error) { reject(new BrokerError('INVALID_JSON', 'Request body is not valid JSON')); }
    });
    request.on('error', reject);
  });
}

function extensionOrigin(request) {
  const origin = request.headers.origin;
  return typeof origin === 'string' && /^chrome-extension:\/\/[a-p]{32}$/.test(origin) ? origin : null;
}

function writeJson(response, status, value, origin) {
  const body = Buffer.from(JSON.stringify(value));
  const headers = {
    'content-type': 'application/json',
    'content-length': body.length,
    'cache-control': 'no-store',
  };
  if (origin) {
    headers['access-control-allow-origin'] = origin;
    headers.vary = 'Origin';
    headers['access-control-allow-headers'] = 'content-type';
    headers['access-control-allow-methods'] = 'GET, POST, OPTIONS';
  }
  response.writeHead(status, headers);
  response.end(body);
}

function sessionProof(request) {
  const value = request.headers['x-saccade-session-token'];
  return typeof value === 'string' ? value : null;
}

function createBrokerServer(state, { port = DEFAULT_PORT, statePath = defaultStatePath() } = {}) {
  const brokerState = state || new BrokerState({ statePath });
  const webSockets = new WebSocketServer({ noServer: true, maxPayload: 1024, perMessageDeflate: false });
  const server = http.createServer(async (request, response) => {
    let responseOrigin;
    try {
      const url = new URL(request.url, `http://127.0.0.1:${port}`);
      const isExtensionRoute = url.pathname.startsWith('/v1/extension/');
      const origin = extensionOrigin(request);
      responseOrigin = origin;
      if (isExtensionRoute && !origin) {
        brokerState.record({
          stage: 'extension_transport', code: 'extension_origin_required',
          method: request.method, route: url.pathname,
        });
        return writeJson(response, 403, { ok: false, error: { code: 'EXTENSION_ORIGIN_REQUIRED', message: 'Extension origin is required' } });
      }
      if (request.method === 'OPTIONS') return writeJson(response, 204, {}, origin);
      if (request.method === 'GET' && url.pathname === '/v1/health') {
        return writeJson(response, 200, { schema: BROKER_SCHEMA, broker_epoch: brokerState.epoch });
      }
      if (request.method === 'GET' && url.pathname === '/v1/doctor') {
        return writeJson(response, 200, brokerState.doctor());
      }
      if (request.method === 'POST' && url.pathname === '/v1/sessions') {
        return writeJson(response, 200, brokerState.createSession(await readBody(request)));
      }
      if (request.method === 'DELETE' && url.pathname.startsWith('/v1/sessions/')) {
        const agentSessionId = decodeURIComponent(url.pathname.slice(13));
        brokerState.authorizeSession(agentSessionId, sessionProof(request));
        return writeJson(response, 200, brokerState.closeSession(agentSessionId));
      }
      if (request.method === 'POST' && url.pathname === '/v1/rpc') {
        const body = await readBody(request);
        brokerState.authorizeSession(body.agent_session_id, sessionProof(request));
        const result = await brokerState.rpc(body.agent_session_id, body.method, body.params, body.timeout_ms, body.request_id);
        return writeJson(response, 200, { ok: true, result });
      }
      if (request.method === 'POST' && url.pathname === '/v1/cancel') {
        const body = await readBody(request);
        brokerState.authorizeSession(body.agent_session_id, sessionProof(request));
        return writeJson(response, 200, brokerState.cancelRequest(body.agent_session_id, body.request_id));
      }
      if (request.method === 'POST' && url.pathname === '/v1/extension/connect') {
        return writeJson(response, 200, brokerState.connectExtension(await readBody(request)), origin);
      }
      if (request.method === 'POST' && url.pathname === '/v1/extension/commands') {
        const body = await readBody(request);
        const commands = await brokerState.pollCommands(
          body.connection_id, EXTENSION_POLL_HEARTBEAT_MS,
        );
        return writeJson(response, 200, { commands, broker_epoch: brokerState.epoch }, origin);
      }
      if (request.method === 'POST' && url.pathname === '/v1/extension/events') {
        const body = await readBody(request);
        return writeJson(response, 200, brokerState.acceptExtensionEvents(body.connection_id, body.events), origin);
      }
      return writeJson(response, 404, { ok: false, error: { code: 'NOT_FOUND', message: 'Broker route not found' } });
    } catch (error) {
      const status = error.code === 'EXTENSION_OFFLINE' ? 503 : 400;
      return writeJson(response, status, {
        ok: false,
        error: { code: error.code || 'BROKER_ERROR', message: error.message, ...error.details },
      }, responseOrigin);
    }
  });
  server.on('upgrade', (request, socket, head) => {
    try {
      const url = new URL(request.url, `http://127.0.0.1:${port}`);
      if (url.pathname !== '/v1/extension/keepalive' || !extensionOrigin(request)) {
        throw new Error('Extension WebSocket origin is required');
      }
      const connectionId = cleanId(url.searchParams.get('connection_id'), 'connection_id');
      const connection = brokerState.connections.get(connectionId);
      if (!connection || connection.state !== 'online') throw new Error('Extension connection is offline');
      webSockets.handleUpgrade(request, socket, head, (webSocket) => {
        if (connection.keepalive_socket && connection.keepalive_socket !== webSocket) {
          try { connection.keepalive_socket.close(1000, 'keepalive replaced'); } catch (_error) { /* closed */ }
        }
        connection.keepalive_socket = webSocket;
        connection.keepalive_connected_at = brokerState.now();
        webSocket.on('message', (data, isBinary) => {
          if (isBinary || data.length > 1024) return webSocket.close(1008, 'invalid heartbeat');
          let message;
          try { message = JSON.parse(data.toString('utf8')); } catch (_error) {
            return webSocket.close(1008, 'invalid heartbeat');
          }
          if (message?.kind !== 'heartbeat') return webSocket.close(1008, 'invalid heartbeat');
          connection.keepalive_count += 1;
          connection.last_seen_at = brokerState.now();
          webSocket.send(JSON.stringify({ kind: 'heartbeat.ack', broker_epoch: brokerState.epoch }));
          return undefined;
        });
        webSocket.on('close', () => {
          if (connection.keepalive_socket === webSocket) connection.keepalive_socket = null;
        });
      });
    } catch (_error) {
      socket.write('HTTP/1.1 403 Forbidden\r\nConnection: close\r\n\r\n');
      socket.destroy();
    }
  });
  return { state: brokerState, server, webSockets, listen: () => new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve(server.address()));
  }) };
}

module.exports = {
  BROKER_SCHEMA,
  DEFAULT_PORT,
  EXTENSION_POLL_HEARTBEAT_MS,
  BrokerError,
  BrokerState,
  createBrokerServer,
  defaultStatePath,
  extensionOrigin,
  materializeDelta,
  projectTruth,
};
