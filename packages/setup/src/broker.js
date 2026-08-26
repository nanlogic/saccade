'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { EventEmitter } = require('node:events');

const BROKER_SCHEMA = 'saccade.node-broker/1';
const DEFAULT_PORT = 32177;
const MAX_BODY_BYTES = 8 * 1024 * 1024;
const HISTORY_LIMIT = 256;
const DIAGNOSTIC_LIMIT = 256;
const COMMAND_LIMIT = 1024;
const EXTENSION_POLL_HEARTBEAT_MS = 2_000;
const STATE_SCHEMA = 'saccade.node-broker-state/1';
const OCCURRENCE_LIMIT = 256;

function opaque(prefix) {
  return `${prefix}_${crypto.randomBytes(24).toString('base64url')}`;
}

function cleanId(value, field) {
  if (typeof value !== 'string' || value.length < 1 || value.length > 256
      || /[\u0000-\u001f\u007f]/.test(value)) throw new Error(`${field} is invalid`);
  return value;
}

function boundedTimeout(value, fallback = 10_000) {
  const timeout = Number.isSafeInteger(value) ? value : fallback;
  if (timeout < 1 || timeout > 60_000) throw new Error('timeout_ms must be between 1 and 60000');
  return timeout;
}

function jsonSize(value) {
  return Buffer.byteLength(JSON.stringify(value));
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
  constructor({ now = () => Date.now(), statePath = null } = {}) {
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

  createSession({ resume_token: resumeProof } = {}) {
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
      session.state = 'online';
      session.last_seen_at = this.now();
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
    const connectionId = opaque('extension');
    for (const connection of this.connections.values()) {
      if (connection.browser_instance_id === browserInstanceId && connection.state === 'online') {
        this.disconnectExtension(connection.connection_id, 'replaced_connection');
      }
    }
    this.connections.set(connectionId, {
      connection_id: connectionId,
      browser_instance_id: browserInstanceId,
      browser_family: payload.browser_family,
      extension_candidate: payload.extension_candidate,
      authorized_tabs: Array.isArray(payload.authorized_tabs) ? payload.authorized_tabs.slice(0, 256) : [],
      state: 'online',
      connected_at: this.now(),
      last_seen_at: this.now(),
      queue: [],
      waiters: [],
    });
    const connection = this.connections.get(connectionId);
    for (const command of this.commands.values()) {
      if (command.state === 'queued' && !command.connection_id && command.idempotent) {
        command.connection_id = connectionId;
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

  disconnectExtension(connectionId, reason = 'disconnected') {
    const connection = this.connections.get(connectionId);
    if (!connection || connection.state !== 'online') return;
    connection.state = 'offline';
    connection.disconnected_at = this.now();
    for (const waiter of connection.waiters.splice(0)) waiter.finish([]);
    for (const command of this.commands.values()) {
      if (command.connection_id !== connectionId || command.state !== 'delivered') continue;
      if (command.idempotent && this.now() < command.deadline_at) {
        command.state = 'queued';
        command.connection_id = null;
      } else {
        this.finishCommand(command, null, new BrokerError(
          'OUTCOME_UNKNOWN',
          'Extension disconnected after command dispatch',
          { outcome: 'outcome_unknown', retry_safe: false },
        ));
      }
    }
    this.record({ stage: 'extension', code: reason, connection_id: connectionId });
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
        connection_id: connection.connection_id,
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
        return {
          tab_id: lease.tab_id,
          browser_instance_id: lease.browser_instance_id || truth?.full?.browser_instance_id,
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
    if (!this.truth.get(tabId)) {
      const available = await this.waitForTruth(tabId, () => true, deadlineAt);
      if (!available) throw new BrokerError(
        'TRUTH_TIMEOUT', 'Canonical Truth did not arrive before the request deadline',
        { stage: 'broker_truth_wait', retry_safe: true },
      );
    }
    if (params.mode === 'delta' && Number.isSafeInteger(params.after_revision)) {
      const current = this.truth.get(tabId);
      if (current && current.revision === params.after_revision) {
        const changed = await this.waitForTruth(tabId, (truth) => (
          truth.document_id !== current.document_id || truth.revision > params.after_revision
        ), deadlineAt);
        if (!changed) return {
          schema: 'saccade.agent-truth/2', mode: 'delta', tab_id: tabId,
          document_id: current.document_id, revision: current.revision,
          complete: true, changes: [], timed_out: true,
          next_basis_revision: current.revision,
        };
      }
    }
    return this.readTruthNow(agentSessionId, params);
  }

  async rpc(agentSessionId, method, params = {}, timeoutMs = 10_000, clientRequestId) {
    this.touchSession(agentSessionId);
    const deadlineAt = this.now() + boundedTimeout(timeoutMs);
    const remaining = () => Math.max(1, deadlineAt - this.now());
    if (method === 'system.capabilities') {
      const tabs = this.listTabs(agentSessionId);
      return {
        schema: 'saccade.capabilities/7', runtime: 'node', broker_schema: BROKER_SCHEMA,
        broker_epoch: this.epoch, agent_session_id: agentSessionId,
        extension_connected: Boolean(this.activeConnection()),
        browser_support: ['chrome', 'edge'], native_host: false, rust: false,
        truth_modes: ['full', 'delta'], exact_tab_routing: true,
        leased_tabs: tabs, current_tab_id: tabs.length === 1 ? tabs[0].tab_id : null,
      };
    }
    if (method === 'tabs.list') return { tabs: this.listTabs(agentSessionId) };
    if (method === 'truth.read') return this.readTruth(agentSessionId, params, deadlineAt);
    if (method === 'tabs.open') {
      if (params.tab_id && params.claim !== 'arm') {
        const existingLease = this.leases.get(cleanId(params.tab_id, 'tab_id'));
        if (existingLease && (existingLease.agent_session_id !== agentSessionId || existingLease.state !== 'active')) {
          throw new BrokerError('TAB_ALREADY_LEASED', 'Tab already has a writer');
        }
      }
      const result = await this.enqueueCommand(agentSessionId, 'tabs.open', params, remaining(), { clientRequestId });
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
      this.requireLease(params.tab_id, agentSessionId);
      const current = this.truth.get(params.tab_id);
      if (!current || current.document_id !== params.document_id || current.revision !== params.basis_revision) {
        throw new BrokerError('STALE_AUTHORITY', 'Action document or revision is stale', { retry_safe: true, current_revision: current?.revision });
      }
      const basisDocumentId = current.document_id;
      const basisRevision = current.revision;
      const inputSteps = params.steps || [params];
      if (!Array.isArray(inputSteps) || inputSteps.length < 1 || inputSteps.length > 32) {
        throw new BrokerError('INVALID_BATCH', 'steps must contain 1 to 32 independent form actions');
      }
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
        const payload = operation === 'type'
          ? { kind: 'text', text: String(step.text ?? step.value ?? '') }
          : operation === 'select'
            ? { kind: 'select', option_object_id: step.option_object_id }
            : { kind: 'none' };
        return {
          browser_instance_id: current.full.browser_instance_id,
          tab_id: params.tab_id, document_id: params.document_id,
          basis_revision: params.basis_revision, object_id: target.object_id,
          action_token: target.action_token, operation, payload,
          timeout_ms: Math.min(boundedTimeout(params.timeout_ms, 5_000), remaining()),
        };
      });
      const batch = Boolean(params.steps);
      const command = batch ? {
        tab_id: params.tab_id, document_id: params.document_id,
        basis_revision: params.basis_revision, timeout_ms: Math.min(boundedTimeout(params.timeout_ms, 5_000), remaining()),
        steps,
      } : steps[0];
      const result = await this.enqueueCommand(agentSessionId, batch ? 'act.batch' : 'act', command, remaining(), {
        clientRequestId, browserInstanceId: current.full.browser_instance_id,
      });
      const finalTruth = result.accepted
        ? await this.waitForTruth(params.tab_id, (truth) => (
          truth.document_id !== basisDocumentId || truth.revision > basisRevision
        ), deadlineAt)
        : null;
      const verified = Boolean(finalTruth);
      const transition = verified && finalTruth.document_id === basisDocumentId
        ? this.readTruthNow(agentSessionId, {
          tab_id: params.tab_id, mode: 'delta', after_revision: basisRevision,
          query: params.query,
        })
        : undefined;
      return {
        command_id: result.command_id,
        outcome: !result.accepted ? 'rejected' : verified ? 'accepted' : 'outcome_unknown',
        occurrence: result.accepted ? (verified ? 'observed' : 'dispatched') : 'not_dispatched',
        semantic_postcondition: {
          code: !result.accepted ? 'rejected' : verified ? 'truth_transition_observed' : 'verification_timeout',
          verified,
        },
        document_id: finalTruth?.document_id || basisDocumentId,
        final_revision: finalTruth?.revision || basisRevision,
        next_basis_revision: finalTruth?.revision || basisRevision,
        relevant_delta: transition,
        steps: batch ? steps.map((step, index) => ({
          object_id: step.object_id,
          operation: step.operation,
          accepted: result.steps?.[index]?.accepted === true,
          verified: Boolean(transition?.changes?.some((change) => change.object_id === step.object_id)),
        })) : undefined,
        retry_safe: !result.accepted,
        external_execution_required: false,
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
    return {
      schema: 'saccade.doctor/2', runtime: 'node', broker_epoch: this.epoch,
      extension_connected: Boolean(this.activeConnection()),
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

function projectTruth(value, query, envelope) {
  const projected = structuredClone(value);
  if (query && Array.isArray(projected.objects)) {
    const roles = new Set(query.roles || []);
    const terms = String(query.text || '').toLowerCase().split(/\s+/).filter(Boolean);
    const limit = Math.min(Number(query.max_objects) || 32, 32);
    const matches = projected.objects.filter((object) => {
      if (roles.size && !roles.has(object.role)) return false;
      const haystack = [object.name, object.text, object.description].filter(Boolean).join(' ').toLowerCase();
      return terms.every((term) => haystack.includes(term));
    });
    projected.objects = matches.slice(0, limit);
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
  const server = http.createServer(async (request, response) => {
    let responseOrigin;
    try {
      const url = new URL(request.url, `http://127.0.0.1:${port}`);
      const isExtensionRoute = url.pathname.startsWith('/v1/extension/');
      const origin = extensionOrigin(request);
      responseOrigin = origin;
      if (isExtensionRoute && !origin) {
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
      if (request.method === 'GET' && url.pathname === '/v1/extension/commands') {
        const commands = await brokerState.pollCommands(
          url.searchParams.get('connection_id'), EXTENSION_POLL_HEARTBEAT_MS,
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
  return { state: brokerState, server, listen: () => new Promise((resolve, reject) => {
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
