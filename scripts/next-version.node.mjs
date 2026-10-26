import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextVersion } from './next-version.mjs';

test('computes patch, minor and major bumps', () => {
  assert.equal(nextVersion('0.2.1', 'patch'), '0.2.2');
  assert.equal(nextVersion('0.2.1', 'minor'), '0.3.0');
  assert.equal(nextVersion('0.2.1', 'major'), '1.0.0');
});

test('accepts an explicit newer version and rejects older, equal or malformed ones', () => {
  assert.equal(nextVersion('0.2.1', '0.10.0'), '0.10.0');
  assert.equal(nextVersion('0.2.1', '1.0.0'), '1.0.0');
  assert.throws(() => nextVersion('0.2.1', '0.2.1'), /not newer/);
  assert.throws(() => nextVersion('0.2.1', '0.2.0'), /not newer/);
  assert.throws(() => nextVersion('0.2.1', '0.1.9'), /not newer/);
  assert.throws(() => nextVersion('0.2.1', 'v0.3.0'), /x\.y\.z/);
  assert.throws(() => nextVersion('0.2.1', ''), /x\.y\.z/);
  assert.throws(() => nextVersion('0.2.1', undefined), /x\.y\.z/);
});
