import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { liveErrorSnapshot } from '../src/ui/live-error.ts';

test('a rejected live invoke does not relabel demo fixture telemetry as live', async () => {
  const main = await readFile(new URL('../src/ui/main.tsx', import.meta.url), 'utf8');
  assert.doesNotMatch(main, /setSnapshot\(\{\s*\.\.\.demoSnapshot,\s*provenance:\s*'live'/s);
});

test('a rejected live invoke produces an empty live error snapshot', () => {
  const snapshot = liveErrorSnapshot('unix:1788007245');
  assert.equal(snapshot.provenance, 'live');
  assert.equal(snapshot.health.status, 'error');
  assert.match(snapshot.health.message, /temporarily unavailable/i);
  assert.deepEqual(snapshot.quotas, []);
  assert.deepEqual(snapshot.models, []);
  assert.deepEqual(snapshot.activity, []);
});
