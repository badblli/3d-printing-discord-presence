import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { bumpVersion } from './bump-version.mjs';

test('bumps all five manifests together and rejects invalid versions without writes', () => {
  const root = mkdtempSync(join(tmpdir(), 'release-bump-'));
  try {
    mkdirSync(join(root, 'src-tauri'));
    const files = {
      'package.json': '{"version":"0.2.0"}\n',
      'package-lock.json': '{"version":"0.2.0","packages":{"":{"version":"0.2.0"}}}\n',
      'src-tauri/Cargo.toml': '[package]\nname = "printpresence"\nversion = "0.2.0"\n\n[dependencies]\nfoo = "1.0"\n',
      'src-tauri/Cargo.lock': '[[package]]\nname = "foo"\nversion = "1.0"\n\n[[package]]\nname = "printpresence"\nversion = "0.2.0"\n',
      'src-tauri/tauri.conf.json': '{"version":"0.2.0"}\n',
    };
    for (const [file, content] of Object.entries(files)) writeFileSync(join(root, file), content);
    assert.throws(() => bumpVersion('0.3', root));
    for (const [file, content] of Object.entries(files)) assert.equal(readFileSync(join(root, file), 'utf8'), content);
    bumpVersion('1.2.3', root);
    for (const file of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']) {
      assert.equal(JSON.parse(readFileSync(join(root, file), 'utf8')).version, '1.2.3');
    }
    assert.equal(JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8')).packages[''].version, '1.2.3');
    assert.match(readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8'), /name = "printpresence"\nversion = "1.2.3"/);
    assert.match(readFileSync(join(root, 'src-tauri/Cargo.lock'), 'utf8'), /name = "printpresence"\nversion = "1.2.3"/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
