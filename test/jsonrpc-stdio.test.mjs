import assert from 'node:assert/strict';
import { once } from 'node:events';
import test from 'node:test';
import { JsonRpcStdioClient, JsonRpcTimeoutError } from '../src/jsonrpc-stdio.mjs';

function fakeChild() {
  const stdin = new (awaitableEmitter)();
  const stdout = new (awaitableEmitter)();
  const stderr = new (awaitableEmitter)();
  let killed = false;
  return { stdin, stdout, stderr, kill: () => { killed = true; stdout.emit('close'); }, get killed() { return killed; } };
}

class awaitableEmitter extends (await import('node:events')).EventEmitter {
  write(value, encoding, callback) { this.emit('write', value); (typeof encoding === 'function' ? encoding : callback)?.(); return true; }
  end() { this.emit('finish'); }
}

test('correlates JSON-RPC request IDs with responses', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child });
  client.start();
  const sent = once(child.stdin, 'write');
  const request = client.request('ping', { value: 1 });
  const [line] = await sent;
  const outbound = JSON.parse(line);
  child.stdout.emit('data', Buffer.from(`${JSON.stringify({ jsonrpc: '2.0', id: outbound.id, result: { pong: true } })}\n`));
  assert.deepEqual(await request, { pong: true });
  await client.close();
});

test('emits notifications separately from responses', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child });
  const notices = [];
  client.on('notification', (notice) => notices.push(notice));
  client.start();
  child.stdout.emit('data', Buffer.from('{"jsonrpc":"2.0","method":"item/updated","params":{"text":"→ tiếng Việt"}}\n'));
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(notices, [{ method: 'item/updated', params: { text: '→ tiếng Việt' } }]);
  await client.close();
});

test('reports malformed JSON lines without crashing', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child });
  const errors = [];
  client.on('protocolError', (error) => errors.push(error));
  client.start();
  child.stdout.emit('data', Buffer.from('not json\n'));
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(errors.length, 1);
  await client.close();
});

test('times out unanswered requests', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child, requestTimeoutMs: 10 });
  client.start();
  await assert.rejects(client.request('slow'), JsonRpcTimeoutError);
  await client.close();
});

test('serializes and deserializes Unicode JSON without charmap conversion', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child });
  client.start();
  const sent = once(child.stdin, 'write');
  const request = client.request('unicode', { text: '→ tiếng Việt' });
  const [line] = await sent;
  const outbound = JSON.parse(line);
  assert.equal(outbound.params.text, '→ tiếng Việt');
  child.stdout.emit('data', Buffer.from(JSON.stringify({ jsonrpc: '2.0', id: outbound.id, result: 'READY' }) + '\n', 'utf8'));
  assert.equal(await request, 'READY');
  await client.close();
});

test('shuts down the child process cleanly', async () => {
  const child = fakeChild();
  const client = new JsonRpcStdioClient({ spawn: () => child });
  client.start();
  await client.close();
  assert.equal(child.killed, true);
});
