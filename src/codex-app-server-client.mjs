import { JsonRpcStdioClient } from './jsonrpc-stdio.mjs';

export class CodexAppServerClient {
  constructor(options = {}) { this.transport = new JsonRpcStdioClient(options); }
  start() { this.transport.start(); return this; }
  close() { return this.transport.close(); }
  onNotification(listener) { this.transport.on('notification', listener); return () => this.transport.off('notification', listener); }
  async initialize() {
    return this.transport.request('initialize', { clientInfo: { name: 'project-orch-native', version: '0.3.0' }, capabilities: { experimentalApi: true } });
  }
  readRateLimits() { return this.transport.request('account/rateLimits/read', {}); }
  listModels() { return this.transport.request('model/list', {}); }
}
