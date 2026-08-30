import { EventEmitter } from 'node:events';
import { spawn as nodeSpawn } from 'node:child_process';

export class JsonRpcTimeoutError extends Error {
  constructor(method, timeoutMs) {
    super(`JSON-RPC request ${method} timed out after ${timeoutMs}ms`);
    this.name = 'JsonRpcTimeoutError';
  }
}

export class JsonRpcRemoteError extends Error {
  constructor(error) {
    super(error?.message ?? 'JSON-RPC remote error');
    this.name = 'JsonRpcRemoteError';
    this.code = error?.code;
    this.data = error?.data;
  }
}

export class JsonRpcStdioClient extends EventEmitter {
  constructor({ command = 'codex', args = ['app-server'], spawn = nodeSpawn, requestTimeoutMs = 30_000 } = {}) {
    super();
    this.command = command;
    this.args = args;
    this.spawn = spawn;
    this.requestTimeoutMs = requestTimeoutMs;
    this.nextId = 1;
    this.pending = new Map();
    this.stderr = '';
    this.stdoutRemainder = '';
    this.child = null;
  }

  start() {
    if (this.child) throw new Error('JSON-RPC client is already started');
    this.child = this.spawn(this.command, this.args, { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    this.child.stdout?.on('data', (chunk) => this.#onStdout(chunk));
    this.child.stderr?.on('data', (chunk) => {
      this.stderr = (this.stderr + Buffer.from(chunk).toString('utf8')).slice(-64 * 1024);
    });
    this.child.on?.('error', (error) => this.#failAll(error));
    this.child.on?.('exit', (code, signal) => {
      if (this.pending.size) this.#failAll(new Error(`app-server exited (${code ?? 'null'}, ${signal ?? 'none'})`));
      this.emit('exit', { code, signal });
    });
    return this;
  }

  request(method, params = undefined, timeoutMs = this.requestTimeoutMs) {
    if (!this.child?.stdin) return Promise.reject(new Error('JSON-RPC client is not started'));
    const id = this.nextId++;
    const message = { jsonrpc: '2.0', id, method };
    if (params !== undefined) message.params = params;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new JsonRpcTimeoutError(method, timeoutMs));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.child.stdin.write(`${JSON.stringify(message)}\n`, 'utf8');
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(error);
      }
    });
  }

  #onStdout(chunk) {
    this.stdoutRemainder += Buffer.from(chunk).toString('utf8');
    const lines = this.stdoutRemainder.split(/\r?\n/);
    this.stdoutRemainder = lines.pop();
    for (const line of lines) {
      if (!line.trim()) continue;
      let message;
      try { message = JSON.parse(line); } catch (error) {
        this.emit('protocolError', new Error(`Malformed JSON-RPC line: ${error.message}`));
        continue;
      }
      if (Object.hasOwn(message, 'id') && (Object.hasOwn(message, 'result') || Object.hasOwn(message, 'error'))) {
        const pending = this.pending.get(message.id);
        if (!pending) { this.emit('protocolError', new Error(`Unmatched JSON-RPC response id: ${message.id}`)); continue; }
        clearTimeout(pending.timer);
        this.pending.delete(message.id);
        if (message.error) pending.reject(new JsonRpcRemoteError(message.error)); else pending.resolve(message.result);
      } else if (typeof message.method === 'string') {
        this.emit('notification', { method: message.method, params: message.params });
      } else {
        this.emit('protocolError', new Error('Invalid JSON-RPC message shape'));
      }
    }
  }

  #failAll(error) {
    for (const pending of this.pending.values()) { clearTimeout(pending.timer); pending.reject(error); }
    this.pending.clear();
  }

  async close({ graceMs = 1_000 } = {}) {
    const child = this.child;
    if (!child) return;
    this.child = null;
    child.stdin?.end?.();
    if (typeof child.once !== 'function') { child.kill?.(); return; }
    const exited = new Promise((resolve) => child.once('exit', resolve));
    const timer = new Promise((resolve) => setTimeout(resolve, graceMs));
    await Promise.race([exited, timer]);
    child.kill?.();
  }
}
