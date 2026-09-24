#!/usr/bin/env node
// Real HTTP/WebSocket lifecycle checks using only the Node 22 standard library.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const binary = resolve(process.argv[2] ?? `${root}/target/release/pikarust-server`);
const output = resolve(process.env.PIKARUST_SERVER_REPORT ?? `${root}/target/e2e/server.json`);
const fen = 'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w - - 0 1';
const movePattern = /^[a-i][0-9][a-i][0-9]$/;
const checks = [];
const sockets = [];
let processLog = '';
let child;
let spawnError;

async function until(check, label, timeout = 5000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (spawnError) throw spawnError;
    if (child?.exitCode !== null && child?.exitCode !== undefined) {
      throw new Error(`server exited (${child.exitCode}) during ${label}`);
    }
    if (await check()) return;
    await delay(30);
  }
  throw new Error(`timed out: ${label}`);
}

async function freePort() {
  const listener = createServer();
  listener.listen(0, '127.0.0.1');
  await once(listener, 'listening');
  const port = listener.address().port;
  await new Promise((done) => listener.close(done));
  return port;
}

class Connection {
  messages = [];
  error;
  constructor(url) {
    this.socket = new WebSocket(url);
    sockets.push(this.socket);
    this.socket.addEventListener('message', (event) => {
      try { this.messages.push(JSON.parse(event.data)); }
      catch (error) { this.error = error; }
    });
    this.socket.addEventListener('error', () => { this.error = new Error('WebSocket transport error'); });
  }
  send(command) { this.socket.send(JSON.stringify(command)); }
  async receive(timeout = 5000) {
    await until(() => {
      if (this.messages.length > 0) return true;
      if (this.error) throw this.error;
      if (!this.messages.length && this.socket.readyState === WebSocket.CLOSED) {
        throw new Error('WebSocket closed before its response');
      }
      return false;
    }, 'WebSocket response', timeout);
    return this.messages.shift();
  }
  async close() {
    this.socket.close();
    await until(() => this.socket.readyState === WebSocket.CLOSED, 'WebSocket close');
  }
  async result(id, timeout = 5000) {
    const deadline = Date.now() + timeout;
    let hasInfo = false;
    while (Date.now() < deadline) {
      const response = await this.receive(deadline - Date.now());
      assert.equal(response.request_id, id);
      if (response.type === 'info') {
        assert.ok(Number.isInteger(response.depth));
        assert.ok(Number.isInteger(response.nodes) && response.nodes >= 0);
        assert.ok(Number.isInteger(response.score_cp));
        assert.ok(Array.isArray(response.pv));
        hasInfo = true;
      } else {
        assert.equal(response.type, 'bestmove');
        assert.ok(hasInfo, 'completed searches must include evaluation information');
        assert.match(response.move, movePattern);
        return response;
      }
    }
    throw new Error(`search ${id} did not complete`);
  }
}

try {
  const port = await freePort();
  const base = `http://127.0.0.1:${port}`;
  child = spawn(binary, [], {
    cwd: root,
    env: { ...process.env, PIKARUST_PORT: String(port), PIKARUST_MAX_ENGINES: '1',
      PIKARUST_MAX_SESSIONS: '1', PIKARUST_THREADS_PER_ENGINE: '1', PIKARUST_HASH_MB: '16',
      PIKARUST_NNUE_FILE: resolve(process.env.PIKARUST_NNUE_FILE ?? `${root}/models/pikafish.nnue`) },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stdout.on('data', (data) => { processLog += data; });
  child.stderr.on('data', (data) => { processLog += data; });
  child.on('error', (error) => { processLog += String(error); spawnError = error; });
  async function request(path, body, status = 200) {
    const response = await fetch(base + path, {
      method: body === undefined ? 'GET' : 'POST',
      headers: { 'content-type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(10000),
    });
    const payload = await response.json();
    assert.equal(response.status, status, JSON.stringify(payload));
    return payload;
  }
  await until(async () => {
    try { return (await request('/api/v1/health')).status === 'ok'; }
    catch { return false; }
  }, 'HTTP readiness', 10000);
  checks.push('HTTP health');
  assert.equal((await request('/api/v1/fen/validate', { fen })).valid, true);
  assert.equal((await request('/api/v1/fen/validate', { fen: 'invalid' })).valid, false);
  for (let count = 0; count < 3; count += 1) {
    await request('/api/v1/bestmove', { fen: 'invalid', depth: 1 }, 400);
    assert.equal((await request('/api/v1/status')).pool_active, 0);
  }
  await request('/api/v1/evaluate', { fen, depth: 0 }, 400);
  const best = await request('/api/v1/bestmove', { fen, depth: 2 });
  assert.match(best.bestmove, movePattern);
  assert.equal(best.depth, 2);
  assert.equal((await request('/api/v1/status')).pool_active, 0);
  checks.push('HTTP valid search, invalid limits, and error-path pool recovery');

  const ws = new Connection(`ws://127.0.0.1:${port}/ws`);
  assert.equal((await ws.receive()).type, 'session');
  const second = new Connection(`ws://127.0.0.1:${port}/ws`);
  assert.equal((await second.receive()).code, 'SESSION_ERROR');
  await second.close();
  await request('/api/v1/bestmove', { fen, depth: 1 }, 503);
  checks.push('Shared HTTP/WebSocket engine and session capacity');
  ws.socket.send('{');
  assert.equal((await ws.receive()).code, 'PARSE_ERROR');
  ws.send({ cmd: 'position', fen });
  ws.send({ cmd: 'go', id: 'invalid', params: { depth: 0 } });
  const invalid = await ws.receive();
  assert.equal(invalid.request_id, 'invalid');
  assert.equal(invalid.code, 'SEARCH_ERROR');
  ws.send({ cmd: 'setoption', id: 'resource', name: 'Threads', value: '128' });
  assert.equal((await ws.receive()).code, 'OPTION_ERROR');
  checks.push('WebSocket malformed input and server-managed resource limits');
  ws.send({ cmd: 'go', id: 'first', params: { infinite: true } });
  ws.send({ cmd: 'stop' });
  await ws.result('first', 3000);
  ws.send({ cmd: 'ucinewgame' });
  ws.send({ cmd: 'position', fen });
  ws.send({ cmd: 'go', id: 'restart', params: { depth: 2 } });
  await ws.result('restart');
  checks.push('WebSocket ordered go/stop, request correlation, and restart');
  for (const [id, side, params] of [
    ['zero-red-clock', 'w', { wtime: 0, btime: 0 }],
    ['zero-black-clock', 'b', { wtime: 0, btime: 0 }],
    ['missing-red-clock', 'w', { btime: 1000 }],
    ['missing-black-clock', 'b', { wtime: 1000 }],
  ]) {
    const clockFen = fen.replace(' w ', ` ${side} `);
    ws.send({ cmd: 'ucinewgame' });
    ws.send({ cmd: 'position', fen: clockFen });
    ws.send({ cmd: 'go', id, params });
    // No stop, depth, node, or move-time limit may rescue these clock searches.
    const result = await ws.result(id, 3000);
    // Apply the returned move through the engine's legality checks and ensure
    // the next side can search, rather than accepting merely coordinate syntax.
    ws.send({ cmd: 'position', fen: clockFen, moves: [result.move] });
    ws.send({ cmd: 'go', id: `${id}-legal`, params: { depth: 1 } });
    await ws.result(`${id}-legal`);
  }
  checks.push('WebSocket exhausted and missing own-side clocks return legal moves without stop');
  ws.send({ cmd: 'go', id: 'disconnect', params: { infinite: true } });
  await delay(100);
  assert.equal((await request('/api/v1/health')).status, 'ok');
  assert.equal((await request('/api/v1/status')).pool_active, 1);
  await ws.close();
  await until(async () => {
    const status = await request('/api/v1/status');
    return status.pool_active === 0 && status.active_sessions === 0;
  }, 'disconnect cancellation and pool recovery');
  assert.match((await request('/api/v1/bestmove', { fen, depth: 1 })).bestmove, movePattern);
  const replacement = new Connection(`ws://127.0.0.1:${port}/ws`);
  assert.equal((await replacement.receive()).type, 'session');
  await replacement.close();
  checks.push('WebSocket disconnect cancels search and restores HTTP/session capacity');
} catch (error) {
  process.exitCode = 1;
  checks.push({ failure: String(error), stack: error.stack });
  console.error(error);
} finally {
  for (const socket of sockets) {
    if (socket.readyState === WebSocket.OPEN) socket.close();
  }
  if (child?.pid && child.exitCode === null) {
    const exited = once(child, 'exit');
    child.kill('SIGINT');
    const force = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exited;
    clearTimeout(force);
  }
  if (child?.pid && child.exitCode !== 0) {
    process.exitCode = 1;
    checks.push({ failure: 'server did not shut down cleanly', exit_code: child.exitCode, signal: child.signalCode });
  }
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, JSON.stringify({ schema_version: 1, passed: !process.exitCode, checks }, null, 2) + '\n');
  await writeFile(output.replace(/\.json$/, '') + '.log', processLog);
  console.log(`${checks.length} server checks; report: ${output}`);
}
