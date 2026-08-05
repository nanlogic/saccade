(() => {
  // The Collector, not an Agent adapter, owns semantic change detection. Fields
  // used only for local authority and native revalidation intentionally do not
  // turn into Truth Layer changes.
  function semanticObject(object) {
    const copy = { ...object };
    delete copy.object_revision;
    delete copy.document_bounds;
    delete copy.viewport_bounds;
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

  const api = Object.freeze({ compileChanges, semanticObject });
  globalThis.SaccadeTruthDelta = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
