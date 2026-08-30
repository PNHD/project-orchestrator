import assert from 'node:assert/strict';
import test from 'node:test';
import { formatTimeLabel } from '../src/ui/time.mjs';

test('formats the native unix timestamp used by live telemetry without throwing', () => {
  assert.match(formatTimeLabel('unix:0', 'en-GB'), /^\d{1,2}:\d{2}$/);
});

test('keeps malformed or absent telemetry timestamps recoverable', () => {
  assert.equal(formatTimeLabel('not-a-timestamp', 'en-GB'), 'Not exposed');
  assert.equal(formatTimeLabel(null, 'en-GB'), 'Not exposed');
});

test('handles unix-prefixed timestamps exclusively with the Unix seconds grammar', () => {
  assert.match(formatTimeLabel('unix:0', 'en-GB'), /^\d{1,2}:\d{2}$/);
  assert.equal(
    formatTimeLabel('unix:999', 'en-GB'),
    new Intl.DateTimeFormat('en-GB', { hour: 'numeric', minute: '2-digit' }).format(new Date(999000)),
  );
  assert.match(formatTimeLabel('unix:1788007245', 'en-GB'), /^\d{1,2}:\d{2}$/);
  for (const value of ['unix:-1', 'unix:1.5', 'unix:abc', 'unix:']) {
    assert.equal(formatTimeLabel(value, 'en-GB'), 'Not exposed');
  }
  assert.doesNotThrow(() => formatTimeLabel('unix:8640000000001', 'en-GB'));
  assert.equal(formatTimeLabel('unix:8640000000001', 'en-GB'), 'Not exposed');
});

test('keeps valid ISO timestamps supported', () => {
  assert.match(formatTimeLabel('2026-08-29T12:34:00Z', 'en-GB'), /^\d{1,2}:\d{2}$/);
});
