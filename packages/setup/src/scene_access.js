'use strict';
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');

function policyPath(environment = process.env) {
  return path.join(environment.SACCADE_STATE_DIR || path.join(os.homedir(), '.saccade'), 'scene-access.json');
}

// Owner-configured scene access, never a public tool argument or page message.
// It permits registered Canvas object frames only, not viewport screenshots.
function readSceneOrigins(file) {
  if (!file) return [];
  try {
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 4096) return [];
    const policy = JSON.parse(fs.readFileSync(file, 'utf8'));
    if (policy.schema !== 'saccade.scene-access/1' || !Array.isArray(policy.origins)
      || policy.origins.length > 16 || Object.keys(policy).some(k => !['schema','origins'].includes(k))) return [];
    const origins = policy.origins.map(value => {
      if (typeof value !== 'string' || value.length > 256) throw Error();
      const url = new URL(value);
      if (!['http:','https:'].includes(url.protocol) || value !== url.origin || url.hostname.includes('*')) throw Error();
      return value;
    });
    return [...new Set(origins)];
  } catch (_) { return []; }
}
function configureSceneAccess(operation, value, file = policyPath()) {
  const current = readSceneOrigins(file);
  if (operation === 'list') return { schema:'saccade.scene-access/1', origins:current };
  const url = new URL(value);
  if (!['allow','revoke'].includes(operation) || !['http:','https:'].includes(url.protocol)
    || value !== url.origin || value.length > 256 || url.hostname.includes('*')) throw new Error('Use one exact HTTP(S) origin, without a path, credentials or wildcard');
  // Do not replace an unreadable existing owner policy with a fresh default.
  if (fs.existsSync(file)) {
    const stat = fs.lstatSync(file);
    if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 4096) throw new Error('Existing scene access policy needs owner review');
    const old = JSON.parse(fs.readFileSync(file,'utf8'));
    if (old.schema !== 'saccade.scene-access/1' || !Array.isArray(old.origins)
      || old.origins.length !== current.length || Object.keys(old).some(k => !['schema','origins'].includes(k))) throw new Error('Existing scene access policy needs owner review');
  }
  const origins = operation === 'allow' ? [...new Set([...current,value])] : current.filter(origin => origin !== value);
  if (origins.length > 16) throw new Error('At most sixteen scene origins may be authorized');
  const policy = {schema:'saccade.scene-access/1', origins};
  fs.mkdirSync(path.dirname(file), {recursive:true});
  fs.writeFileSync(file, JSON.stringify(policy,null,2)+'\n', {mode:0o600});
  return policy;
}
module.exports = { readSceneOrigins, configureSceneAccess, policyPath };
