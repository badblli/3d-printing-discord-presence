import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function bumpVersion(version, directory = root) {
  if (!semver.test(version)) throw new Error('Expected a version in x.y.z format');
  const files = [
    'package.json',
    'package-lock.json',
    'src-tauri/Cargo.toml',
    'src-tauri/Cargo.lock',
    'src-tauri/tauri.conf.json',
  ];
  const updates = files.map(file => {
    const path = join(directory, file);
    const original = readFileSync(path, 'utf8');
    let next;
    if (file.endsWith('.json')) {
      const data = JSON.parse(original);
      data.version = version;
      if (file === 'package-lock.json') {
        if (!data.packages?.['']) throw new Error('Missing root package lock entry');
        data.packages[''].version = version;
      }
      // Replace only version fields so each release does not reformat manifests.
      let replacements = 0;
      next = original.replace(/("version"[ \t]*:[ \t]*")[^"]+(")/g, (match, before, after) => {
        if (replacements === (file === 'package-lock.json' ? 2 : 1)) return match;
        replacements++;
        return `${before}${version}${after}`;
      });
      if (replacements !== (file === 'package-lock.json' ? 2 : 1)) throw new Error(`Version field missing in ${file}`);
    } else if (file.endsWith('Cargo.toml')) {
      if (!/^\[package\]\r?\nname = "printpresence"\r?\nversion = "[^"]+"/m.test(original)) throw new Error('Cargo package not found');
      next = original.replace(/^(\[package\]\r?\nname = "printpresence"\r?\nversion = ")[^"]+(".*)$/m, `$1${version}$2`);
    } else {
      const pattern = /^(\[\[package\]\]\r?\nname = "printpresence"\r?\nversion = ")[^"]+(".*)$/m;
      if (!pattern.test(original)) throw new Error('Cargo lock package not found');
      next = original.replace(pattern, `$1${version}$2`);
    }
    return [path, next];
  });
  for (const [path, contents] of updates) writeFileSync(path, contents);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    bumpVersion(process.argv[2]);
    process.stdout.write(`Version set to ${process.argv[2]}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
