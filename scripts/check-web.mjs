#!/usr/bin/env node
// Exercise the production frontend, real bridge, and native NNUE engine together.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { access, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { createServer } from 'node:net';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const frontend = resolve(root, 'pikarust-web/frontend');
const bundle = resolve(root, 'pikarust-web/dist');
const output = resolve(process.env.PIKARUST_WEB_REPORT_DIR ?? `${root}/target/e2e/web`);
const requireFrontend = createRequire(`${frontend}/package.json`);
const { chromium, expect: baseExpect } = requireFrontend('@playwright/test');
const expect = baseExpect.configure({ timeout: 10000 });
const checks = [];
const frames = [];
const errors = [];
let bridge;
let bridgeLog = '';
let browser;
let page;
let port;
let expectedDisconnect = false;
let failure;

async function hashFile(path) {
  return createHash('sha256').update(await readFile(path)).digest('hex');
}

async function allocatePort() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const assigned = server.address().port;
  await new Promise((done) => server.close(done));
  return assigned;
}

async function startBridge() {
  let spawnError;
  bridge = spawn(resolve(bundle, 'pikarust-bridge'), [
    '--engine-path', resolve(bundle, 'pikarust'), '--static-dir', bundle, '--port', String(port),
  ], {
    cwd: bundle,
    env: { ...process.env, PIKARUST_NNUE_FILE: resolve(bundle, 'models/pikafish.nnue') },
    detached: process.platform !== 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  bridge.stdout.on('data', (data) => { bridgeLog += data; });
  bridge.stderr.on('data', (data) => { bridgeLog += data; });
  bridge.on('error', (error) => { spawnError = error; });
  await expect.poll(async () => {
    if (spawnError) throw spawnError;
    if (bridge.exitCode !== null) throw new Error(`bridge exited: ${bridge.exitCode}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}`, { signal: AbortSignal.timeout(1000) });
      return response.ok;
    } catch { return false; }
  }, { message: 'production bridge serves the built frontend' }).toBe(true);
}

async function stopBridge() {
  const child = bridge;
  if (!child?.pid) return;
  const running = child.exitCode === null && child.signalCode === null;
  const exited = running ? once(child, 'exit') : Promise.resolve();
  bridge = undefined;
  try {
    if (process.platform === 'win32') {
      if (running) execFileSync('taskkill', ['/pid', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } else {
      // A bridge connection owns a native engine process. Reap the entire test
      // process group, including a currently thinking engine, on every exit path.
      process.kill(-child.pid, 'SIGTERM');
    }
  } catch (error) { if (error.code !== 'ESRCH') throw error; }
  const timer = setTimeout(() => {
    try {
      if (process.platform === 'win32') child.kill('SIGKILL');
      else process.kill(-child.pid, 'SIGKILL');
    } catch (error) { if (error.code !== 'ESRCH') throw error; }
  }, 3000);
  try { await exited; } finally { clearTimeout(timer); }
}

const state = () => page.getByTestId('game-state');
const entries = () => page.getByTestId('move-entry');

async function history() {
  return entries().evaluateAll((nodes) => nodes.map((node) => node.getAttribute('data-move')));
}

async function settled(count, side) {
  await expect(entries()).toHaveCount(count);
  const phase = await state().getAttribute('data-phase');
  await expect(state()).toHaveAttribute('data-status', phase === 'idle' ? 'idle' : 'ready');
  await expect(state()).toHaveAttribute('data-thinking', 'false');
  if (side) await expect(state()).toHaveAttribute('data-current-side', side);
  await assertBoardMatchesHistory();
}

async function clickSquare(square) {
  const board = page.getByTestId('board');
  await board.scrollIntoViewIfNeeded();
  const bounds = await board.boundingBox();
  assert.ok(bounds, 'visible board has bounds');
  const flipped = await board.getAttribute('data-flipped') === 'true';
  const col = square.charCodeAt(0) - 97;
  const row = 9 - Number(square[1]);
  const visualCol = flipped ? 8 - col : col;
  const visualRow = flipped ? 9 - row : row;
  await board.click({ position: {
    x: ((visualCol + 0.8) / 9.6) * bounds.width,
    y: ((visualRow + 0.8) / 10.6) * bounds.height,
  } });
}

async function playMove(side, waitForReply = true) {
  await expect(state()).toHaveAttribute('data-status', 'ready');
  await expect(state()).toHaveAttribute('data-player-side', side);
  await expect(state()).toHaveAttribute('data-current-side', side);
  await expect(state()).toHaveAttribute('data-thinking', 'false');
  const before = await entries().count();
  const pieces = await page.getByTestId('piece').evaluateAll((nodes) => nodes.map((node) => ({
    square: node.getAttribute('data-square'), piece: node.getAttribute('data-piece'),
  })));
  const own = pieces.filter(({ piece }) => (piece === piece.toUpperCase()) === (side === 'w'));
  own.sort((a, b) => 'PNRCBAK'.indexOf(a.piece.toUpperCase()) - 'PNRCBAK'.indexOf(b.piece.toUpperCase()));
  for (const piece of own) {
    await clickSquare(piece.square);
    const targets = page.getByTestId('legal-target');
    if (await targets.count() === 0) continue;
    const destination = await targets.first().getAttribute('data-square');
    await clickSquare(destination);
    if (waitForReply) await settled(before + 2, side);
    else {
      await expect(entries()).toHaveCount(before + 1);
      await expect(state()).toHaveAttribute('data-thinking', 'true');
    }
    return { from: piece.square, to: destination, before };
  }
  throw new Error(`no selectable legal ${side} move in ${JSON.stringify(await history())}`);
}

async function assertBoardMatchesHistory() {
  const rows = 'rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR'.split('/');
  const expected = new Map();
  for (const [row, rank] of rows.entries()) {
    let file = 0;
    for (const piece of rank) {
      if (/\d/.test(piece)) file += Number(piece);
      else { expected.set(`${String.fromCharCode(97 + file)}${9 - row}`, piece); file += 1; }
    }
  }
  for (const [ply, move] of (await history()).entries()) {
    assert.match(move, /^[a-i][0-9][a-i][0-9]$/);
    const from = move.slice(0, 2);
    const to = move.slice(2);
    const piece = expected.get(from);
    assert.ok(piece, `recorded move ${move} has an origin piece`);
    assert.equal(piece === piece.toUpperCase(), ply % 2 === 0, 'recorded sides alternate');
    expected.delete(from);
    expected.set(to, piece);
  }
  const displayed = await page.getByTestId('piece').evaluateAll((nodes) => nodes.map((node) => [
    node.getAttribute('data-square'), node.getAttribute('data-piece'),
  ]));
  assert.deepEqual(displayed.sort(), [...expected.entries()].sort(), 'rendered board matches the move list');
}

async function checkpoint(name, action) {
  const started = Date.now();
  try {
    await action();
    checks.push({ name, passed: true, duration_ms: Date.now() - started });
    console.log(`PASS ${name}`);
  } catch (error) {
    checks.push({ name, passed: false, duration_ms: Date.now() - started, error: String(error) });
    throw error;
  }
}

async function newGame() {
  await page.getByRole('button', { name: '新对局', exact: true }).click();
  await expect(state()).toHaveAttribute('data-phase', 'idle');
  await settled(0, 'w');
}

async function depthGame(side) {
  await page.getByRole('button', { name: side === 'w' ? '红方' : '黑方', exact: true }).click();
  await page.getByLabel('Search mode').selectOption('depth');
  await page.getByLabel('Search depth').selectOption('6');
  await page.getByRole('button', { name: '开始对局', exact: true }).click();
  await expect(state()).toHaveAttribute('data-phase', 'playing');
  await settled(side === 'w' ? 0 : 1, side);
}

function hasFrame(direction, pattern, since = 0) {
  return frames.slice(since).some((frame) => frame.direction === direction
    && frame.payload.split('\n').some((line) => pattern.test(line.trim())));
}

async function waitForFrame(direction, pattern, since) {
  await expect.poll(() => hasFrame(direction, pattern, since), {
    message: `observe ${direction} UCI ${pattern}`,
  }).toBe(true);
}

async function longRedGame() {
  await page.getByRole('button', { name: '红方', exact: true }).click();
  await page.getByLabel('Search mode').selectOption('movetime');
  await page.getByLabel('Move time').selectOption('30000');
  await page.getByRole('button', { name: '开始对局', exact: true }).click();
  await expect(state()).toHaveAttribute('data-phase', 'playing');
  await settled(0, 'w');
}

function optionalFont(url) {
  try { return ['fonts.googleapis.com', 'fonts.gstatic.com'].includes(new URL(url).hostname); }
  catch { return false; }
}

function expectedSocketError(message) {
  return expectedDisconnect && message.includes(`ws://127.0.0.1:${port}/ws`)
    && /WebSocket connection .* failed:/.test(message)
    && /ERR_CONNECTION_REFUSED|ERR_CONNECTION_RESET|closed before the connection is established/.test(message);
}

try {
  await mkdir(output, { recursive: true });
  // A failed rerun must never preserve a screenshot or report from an older run.
  for (const name of ['report.json', 'protocol.jsonl', 'bridge.log', 'provenance.json', 'board.png']) {
    await rm(resolve(output, name), { force: true });
  }
  for (const path of ['index.html', 'pikarust-bridge', 'pikarust', 'models/pikafish.nnue']) {
    await access(resolve(bundle, path));
  }
  // Fail before launching an engine if the matching locked browser is missing.
  await access(chromium.executablePath());
  port = await allocatePort();
  await startBridge();
  browser = await chromium.launch({ headless: true });
  page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, locale: 'zh-CN' });
  page.on('pageerror', (error) => errors.push({ type: 'pageerror', message: String(error) }));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push({
      type: 'console', message: message.text(), url: message.location().url,
      expected: expectedSocketError(message.text())
        || (optionalFont(message.location().url) && message.text().startsWith('Failed to load resource:')),
    });
  });
  page.on('requestfailed', (request) => errors.push({
    type: 'requestfailed', message: request.failure()?.errorText, url: request.url(),
    expected: optionalFont(request.url()),
  }));
  page.on('response', (response) => {
    if (response.status() >= 400) errors.push({
      type: 'http', message: `HTTP ${response.status()}`, url: response.url(),
      expected: optionalFont(response.url()),
    });
  });
  page.on('websocket', (socket) => {
    socket.on('framesent', ({ payload }) => frames.push({ direction: 'sent', payload: String(payload) }));
    socket.on('framereceived', ({ payload }) => frames.push({ direction: 'received', payload: String(payload) }));
  });
  await page.goto(`http://127.0.0.1:${port}`, { waitUntil: 'domcontentloaded' });
  await checkpoint('Production page and NNUE-backed engine handshake', async () => {
    await expect(page.getByText('Connected', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: '开始对局', exact: true })).toBeEnabled();
    await expect(page.getByTestId('board')).toBeVisible();
    await settled(0, 'w');
  });
  await checkpoint('Play three red turns with real engine replies', async () => {
    await depthGame('w');
    for (let turn = 0; turn < 3; turn += 1) await playMove('w');
    await settled(6, 'w');
    assert.ok(frames.some((frame) => frame.direction === 'sent' && frame.payload === 'go depth 6'));
  });
  await checkpoint('Undo and flip preserve board/history agreement', async () => {
    await page.getByRole('button', { name: '悔棋', exact: true }).click();
    await settled(4, 'w');
    const before = await history();
    await page.getByRole('button', { name: /翻转棋盘/ }).click();
    await expect(page.getByTestId('board')).toHaveAttribute('data-flipped', 'true');
    await assertBoardMatchesHistory();
    assert.deepEqual(await history(), before);
  });
  await checkpoint('Play three black turns on the flipped board', async () => {
    await newGame();
    await depthGame('b');
    await expect(page.getByRole('button', { name: '悔棋', exact: true })).toBeDisabled();
    await expect(page.getByTestId('board')).toHaveAttribute('data-flipped', 'true');
    for (let turn = 0; turn < 3; turn += 1) await playMove('b');
    await settled(7, 'b');
  });
  await checkpoint('Changing search mode uses its unchanged default value', async () => {
    await newGame();
    await page.getByLabel('Search mode').selectOption('movetime');
    await expect(page.getByLabel('Move time')).toHaveValue('3000');
    const since = frames.length;
    await page.getByRole('button', { name: '开始对局', exact: true }).click();
    await waitForFrame('sent', /^go movetime 3000$/, since);
    await settled(1, 'b');
  });
  await checkpoint('Undo cancels an active search and discards its result', async () => {
    await newGame();
    await longRedGame();
    const since = frames.length;
    await playMove('w', false);
    await waitForFrame('sent', /^go movetime 30000$/, since);
    await page.getByRole('button', { name: '悔棋', exact: true }).click();
    await waitForFrame('sent', /^stop$/, since);
    await waitForFrame('received', /^bestmove /, since);
    await settled(0, 'w');
  });
  await checkpoint('New game cancels search and can immediately switch sides', async () => {
    const since = frames.length;
    await playMove('w', false);
    await waitForFrame('sent', /^go movetime 30000$/, since);
    await newGame();
    await depthGame('b');
    await waitForFrame('sent', /^stop$/, since);
    await waitForFrame('received', /^bestmove /, since);
    await settled(1, 'b');
  });
  await checkpoint('Reconnect restores the position and permits continued play', async () => {
    await playMove('b');
    const before = await history();
    expectedDisconnect = true;
    await stopBridge();
    await expect(state()).toHaveAttribute('data-connected', 'false');
    await expect(state()).toHaveAttribute('data-status', 'disconnected');
    assert.deepEqual(await history(), before, 'disconnect preserves the full move history');
    await startBridge();
    await expect(state()).toHaveAttribute('data-connected', 'true');
    await settled(before.length, 'b');
    expectedDisconnect = false;
    assert.deepEqual(await history(), before, 'reconnect restores the same position');
    await playMove('b');
  });
  await checkpoint('Reconnect resumes an interrupted search and still permits undo', async () => {
    await newGame();
    await longRedGame();
    await playMove('w', false);
    const before = await history();
    expectedDisconnect = true;
    await stopBridge();
    await expect(state()).toHaveAttribute('data-status', 'disconnected');
    assert.deepEqual(await history(), before);
    const since = frames.length;
    await startBridge();
    await expect(state()).toHaveAttribute('data-connected', 'true');
    await waitForFrame('sent', /^go movetime 30000$/, since);
    await expect(state()).toHaveAttribute('data-status', 'thinking');
    expectedDisconnect = false;
    assert.deepEqual(await history(), before);
    await page.getByRole('button', { name: '悔棋', exact: true }).click();
    await waitForFrame('sent', /^stop$/, since);
    await waitForFrame('received', /^bestmove /, since);
    await settled(0, 'w');
  });
  await checkpoint('Recovered game remains playable without runtime errors', async () => {
    await newGame();
    await depthGame('w');
    await playMove('w');
    assert.deepEqual(errors.filter((error) => !error.expected), [], 'no browser or application request errors');
    assert.equal(hasFrame('received', /^info string error\b/i), false, 'no native engine errors');
  });
} catch (error) {
  failure = error;
  process.exitCode = 1;
  console.error(error);
} finally {
  if (page) {
    try { await page.screenshot({ path: resolve(output, 'board.png'), fullPage: true }); }
    catch (error) { failure ??= error; process.exitCode = 1; errors.push({ type: 'screenshot', message: String(error) }); }
  }
  if (!failure && errors.some((error) => !error.expected)) {
    failure = new Error('Unexpected browser errors occurred; see report.json');
    process.exitCode = 1;
  }
  for (const cleanup of [async () => browser?.close(), stopBridge]) {
    try { await cleanup(); }
    catch (error) { failure ??= error; process.exitCode = 1; errors.push({ type: 'cleanup', message: String(error) }); }
  }
  await mkdir(output, { recursive: true });
  await writeFile(resolve(output, 'report.json'), JSON.stringify({
    schema_version: 1, passed: !failure, checks, errors,
    failure: failure ? { message: String(failure), stack: failure.stack } : null,
  }, null, 2) + '\n');
  await writeFile(resolve(output, 'protocol.jsonl'), frames.map((frame) => JSON.stringify(frame)).join('\n') + '\n');
  await writeFile(resolve(output, 'bridge.log'), bridgeLog);
  const inputs = {};
  for (const [name, path] of Object.entries({
    engine: resolve(bundle, 'pikarust'), bridge: resolve(bundle, 'pikarust-bridge'),
    nnue: resolve(bundle, 'models/pikafish.nnue'), frontend: resolve(bundle, 'index.html'),
    frontend_lock: resolve(frontend, 'package-lock.json'),
  })) {
    try { inputs[`${name}_sha256`] = await hashFile(path); }
    catch (error) { inputs[`${name}_error`] = String(error); }
  }
  await writeFile(resolve(output, 'provenance.json'), JSON.stringify({
    commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    node: process.version, playwright: requireFrontend('@playwright/test/package.json').version,
    ...inputs,
  }, null, 2) + '\n');
  console.log(`Browser checks: ${checks.filter((check) => check.passed).length}/${checks.length}; report: ${output}`);
}
