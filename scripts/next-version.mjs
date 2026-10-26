import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function parse(version) {
  const match = semver.exec(version ?? '');
  if (!match) throw new Error(`Expected a version in x.y.z format, got "${version}"`);
  return match.slice(1).map(Number);
}

// request is "patch", "minor", "major" or an explicit x.y.z that must be newer than current.
export function nextVersion(current, request) {
  const [major, minor, patch] = parse(current);
  let next;
  if (request === 'major') next = `${major + 1}.0.0`;
  else if (request === 'minor') next = `${major}.${minor + 1}.0`;
  else if (request === 'patch') next = `${major}.${minor}.${patch + 1}`;
  else next = request;
  const parts = parse(next);
  const order = parts.map((part, i) => part - [major, minor, patch][i]).find(diff => diff !== 0) ?? 0;
  if (order <= 0) throw new Error(`Version ${next} is not newer than ${current}`);
  return next;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const current = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version;
    process.stdout.write(`${nextVersion(current, process.argv[2])}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
