(() => {
  // The Collector, not an Agent adapter, owns Truth change detection. Current
  // document/viewport geometry is public browser truth, so movement and resize
  // must update the same stable object identity. Only local authority fields are
  // excluded from the public fingerprint.
  function semanticObject(object) {
    const copy = { ...object };
    delete copy.object_revision;
    delete copy.action_token;
    delete copy.loop_class_token;
    copy.actionable = Boolean(object.action_token);
    return copy;
  }

  function fingerprint(object) {
    return JSON.stringify(semanticObject(object));
  }

  function compileChanges(previousObjects, currentObjects) {
    if (!previousObjects) return [];
    const previous = new Map(previousObjects.map((object) => [object.object_id, object]));
    const current = new Map(currentObjects.map((object) => [object.object_id, object]));
    const changes = [];
    for (const object of currentObjects) {
      const before = previous.get(object.object_id);
      if (!before) {
        changes.push({ kind: 'appeared', object_id: object.object_id, object_revision: object.object_revision });
      } else if (fingerprint(before) !== fingerprint(object)) {
        changes.push({ kind: 'updated', object_id: object.object_id, object_revision: object.object_revision });
      }
    }
    for (const object of previousObjects) {
      if (!current.has(object.object_id)) {
        changes.push({ kind: 'disappeared', object_id: object.object_id, object_revision: object.object_revision });
      }
    }
    return changes;
  }

  // Native Messaging transports the first revision as a complete snapshot.
  // Later revisions carry complete values only for changed identities, plus
  // the opaque current authorities of unchanged actionable objects. The Host
  // can therefore materialize current Truth without receiving the whole page
  // again or treating authority rotation as semantic change.
  function compactTransport(currentObjects, changes) {
    const current = new Map(currentObjects.map((object) => [object.object_id, object]));
    const changedIds = new Set();
    const objects = [];
    for (const change of changes) {
      if (changedIds.has(change.object_id)) throw new Error('delta repeats an object identity');
      changedIds.add(change.object_id);
      if (change.kind === 'disappeared') {
        if (current.has(change.object_id)) throw new Error('disappeared delta object is still current');
        continue;
      }
      const object = current.get(change.object_id);
      if (!object || object.object_revision !== change.object_revision) {
        throw new Error('delta does not carry the changed current object');
      }
      objects.push(object);
    }
    const authorities = currentObjects
      .filter((object) => !changedIds.has(object.object_id) && object.action_token)
      .map((object) => ({ object_id: object.object_id, action_token: object.action_token }));
    return { objects, authorities };
  }

  const api = Object.freeze({ compileChanges, compactTransport, semanticObject });
  globalThis.SaccadeTruthDelta = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
