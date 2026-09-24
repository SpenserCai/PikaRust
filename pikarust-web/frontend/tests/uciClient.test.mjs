import assert from 'node:assert/strict';
import { test } from 'node:test';
import { UciClient, CancelledRequest, parseInfo } from '../src/lib/uciClient.ts';

function fixture(t) {
  const sent = [], readiness = [], info = [];
  const client = new UciClient({ ready: value => readiness.push(value), info: value => info.push(value), failure: error => { throw error; } });
  client.attach(command => sent.push(command));
  t.after(() => client.disconnect());
  return { client, sent, readiness, info };
}
function handshake(client) { client.accept('uciok'); client.accept('readyok'); }
function inspected(client, moves = ['a0a1'], checked = false, gameStatus = moves.length ? 'ongoing' : 'loss') {
  for (const move of moves) client.accept(`${move}: 1`);
  client.accept(`Nodes searched: ${moves.length}`);
  client.accept(checked ? 'Final evaluation: none (in check)' : 'Final evaluation +262 (side to move, internal units)');
  if (gameStatus !== null) client.accept(`Game status: ${gameStatus}`);
  client.accept('readyok');
}
const settings = { mode: 'depth', depth: 6, movetime: 1000 };

test('socket open is not readiness; complete UCI handshake is required', async t => {
  const { client, sent, readiness } = fixture(t);
  await assert.rejects(client.inspect('position startpos'), /尚未连接/);
  client.accept('uciok');
  assert.deepEqual(sent, ['setoption name UCI_ShowWDL value true', 'isready']);
  assert.deepEqual(readiness, [false]);
  client.accept('readyok');
  assert.equal(readiness.at(-1), true);
});

test('legal moves and check status are supplied by strict engine diagnostics', async t => {
  const { client, sent } = fixture(t);
  handshake(client);
  const result = client.inspect('position startpos', true);
  assert.deepEqual(sent.slice(-6), ['ucinewgame', 'position startpos', 'go perft 1', 'eval', 'd', 'isready']);
  inspected(client, ['a0a1', 'b0c2'], true);
  assert.deepEqual(await result, { legalMoves: ['a0a1', 'b0c2'], inCheck: true, gameStatus: 'ongoing' });
  const terminal = client.inspect('position fen terminal');
  inspected(client, [], false);
  assert.deepEqual(await terminal, { legalMoves: [], inCheck: false, gameStatus: 'loss' });
});

test('missing or invalid diagnostic output cannot masquerade as a legal position', async t => {
  const { client } = fixture(t);
  handshake(client);
  const incomplete = client.inspect('position startpos');
  client.accept('a0a1: 1');
  client.accept('readyok');
  await assert.rejects(incomplete, /完整/);
  const badPosition = client.inspect('position fen bad');
  client.accept('info string error: invalid FEN');
  inspected(client);
  await assert.rejects(badPosition, /invalid FEN/);
});

test('cancel drains the old search before starting a new position; stale output is ignored', async t => {
  const { client, sent, info } = fixture(t);
  handshake(client);
  const old = client.search('position startpos', settings);
  const rejected = assert.rejects(old, CancelledRequest);
  client.cancel();
  assert.deepEqual(sent.slice(-2), ['stop', 'isready']);
  const next = client.inspect('position startpos moves b0c2');
  assert.notEqual(sent.at(-1), 'position startpos moves b0c2');
  const infoCount = info.length;
  client.accept('info depth 99 score cp 500 pv a0a1');
  client.accept('bestmove a0a1');
  assert.equal(info.length, infoCount);
  client.accept('readyok');
  assert.deepEqual(sent.slice(-5), ['position startpos moves b0c2', 'go perft 1', 'eval', 'd', 'isready']);
  inspected(client, ['b9c7']);
  assert.deepEqual(await next, { legalMoves: ['b9c7'], inCheck: false, gameStatus: 'ongoing' });
  await rejected;
});

test('StrictMode-style setup cleanup setup cannot resolve the new request with old data', async t => {
  const { client } = fixture(t);
  handshake(client);
  const first = client.inspect('position startpos');
  const rejected = assert.rejects(first, CancelledRequest);
  client.cancel();
  const second = client.inspect('position startpos moves b0c2');
  inspected(client, ['a0a1']);
  inspected(client, ['b9c7']);
  await rejected;
  assert.deepEqual(await second, { legalMoves: ['b9c7'], inCheck: false, gameStatus: 'ongoing' });
});

test('disconnect rejects pending work; reconnect replays a new independent request', async t => {
  const { client, sent } = fixture(t);
  handshake(client);
  const old = client.search('position startpos', settings);
  const rejected = assert.rejects(old, CancelledRequest);
  client.disconnect();
  await rejected;
  client.attach(command => sent.push(command));
  handshake(client);
  const next = client.search('position startpos moves b0c2', { ...settings, mode: 'movetime' });
  assert.equal(sent.at(-1), 'go movetime 1000');
  client.accept('bestmove b9c7');
  assert.equal(sent.at(-1), 'isready');
  client.accept('readyok');
  assert.equal(await next, 'b9c7');
});

test('rejected search is drained with stop + ready and exposes the engine error', async t => {
  const { client, sent } = fixture(t);
  handshake(client);
  const result = client.search('position fen bad', settings);
  client.accept('info string error: illegal move');
  assert.deepEqual(sent.slice(-2), ['stop', 'isready']);
  client.accept('bestmove a0a1');
  client.accept('readyok');
  await assert.rejects(result, /illegal move/);
});

test('info parser handles score, WDL and PV without accepting nonnumeric scores', () => {
  assert.deepEqual(parseInfo('info depth 6 nodes 123 score cp -42 wdl 1 2 997 pv a0a1 b9c7'), {
    depth: 6, nodes: 123, score: { cp: -42 }, wdl: [1, 2, 997], pv: 'a0a1 b9c7',
  });
  assert.deepEqual(parseInfo('info score cp nonsense'), {});
});

test('terminal status is authoritative and missing or inconsistent status is rejected', async t => {
  const { client } = fixture(t);
  handshake(client);
  const draw = client.inspect('position startpos moves a0a1 a9a8 a1a0 a8a9 a0a1 a9a8 a1a0 a8a9');
  inspected(client, ['a0a1'], false, 'draw');
  assert.deepEqual(await draw, { legalMoves: ['a0a1'], inCheck: false, gameStatus: 'draw' });
  for (const [moves, status] of [[['a0a1'], null], [[], 'ongoing']]) {
    const invalid = client.inspect('position startpos');
    inspected(client, moves, false, status);
    await assert.rejects(invalid, /完整/);
  }
});
