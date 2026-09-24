import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createGameState, gameReducer, undoTarget, searchCommand, DEFAULT_SETTINGS } from '../src/lib/game.ts';

const reduce = gameReducer;
function started(side = 'w') {
  return reduce(reduce(createGameState(), { type: 'SET_PLAYER_SIDE', side }), { type: 'START_GAME' });
}
function inspect(state, legalMoves, inCheck = false, gameStatus = legalMoves.length ? 'ongoing' : 'loss') {
  return reduce(state, { type: 'INSPECTED', revision: state.revision, legalMoves, inCheck, gameStatus });
}
function human(state, move) { return reduce(inspect(state, [move]), { type: 'PLAYER_MOVE', move }); }
function engine(state, move) { return reduce(inspect(state, [move]), { type: 'ENGINE_MOVE', revision: state.revision, move }); }

test('switching search mode immediately uses that mode and retains both values', () => {
  assert.equal(searchCommand({ ...DEFAULT_SETTINGS, mode: 'movetime' }), 'go movetime 3000');
  const settings = { mode: 'movetime', depth: 6, movetime: 1000 };
  assert.equal(searchCommand({ ...settings, mode: 'depth' }), 'go depth 6');
});

test('human moves require an authoritative legal move and a ready human turn', () => {
  const state = started();
  assert.equal(reduce(state, { type: 'PLAYER_MOVE', move: 'a0a9' }), state);
  const ready = inspect(state, ['a0a1']);
  assert.equal(reduce(ready, { type: 'PLAYER_MOVE', move: 'a0a9' }), ready);
  const moved = reduce(ready, { type: 'PLAYER_MOVE', move: 'a0a1' });
  assert.equal(moved.currentSide, 'b');
  assert.equal(reduce(moved, { type: 'PLAYER_MOVE', move: 'a0a1' }), moved);
});

test('undo during the third ply removes only the pending human move', () => {
  const previous = engine(human(started(), 'b0c2'), 'b9c7');
  const thinking = human(previous, 'h0g2');
  const undone = reduce(thinking, { type: 'UNDO' });
  assert.deepEqual(undone.moveHistory, ['b0c2', 'b9c7']);
  assert.equal(undone.currentSide, 'w');
  assert.ok(undone.revision > thinking.revision);
});

test('undo after an answer returns to the most recent human decision', () => {
  const completed = engine(human(started(), 'b0c2'), 'b9c7');
  assert.deepEqual(reduce(completed, { type: 'UNDO' }).moveHistory, []);
  const opening = engine(started('b'), 'b0c2');
  assert.equal(undoTarget(opening), null);
  assert.equal(reduce(opening, { type: 'UNDO' }), opening);
  const round = engine(human(opening, 'b9c7'), 'h0g2');
  const undone = reduce(round, { type: 'UNDO' });
  assert.deepEqual(undone.moveHistory, ['b0c2']);
  assert.equal(undone.currentSide, 'b');
});

test('new games reject stale inspections and bestmoves even after starting again', () => {
  const pending = inspect(human(started(), 'b0c2'), ['b9c7']);
  const fresh = reduce(reduce(pending, { type: 'NEW_GAME' }), { type: 'START_GAME' });
  assert.equal(reduce(fresh, { type: 'ENGINE_MOVE', revision: pending.revision, move: 'b9c7' }), fresh);
  assert.equal(reduce(fresh, { type: 'INSPECTED', revision: pending.revision, legalMoves: [], inCheck: true, gameStatus: 'loss' }), fresh);
  assert.deepEqual(fresh.moveHistory, []);
});

test('disconnect keeps the position and side, blocks moves, and can resume the AI turn', () => {
  const pending = inspect(human(started(), 'b0c2'), ['b9c7']);
  const disconnected = reduce(pending, { type: 'DISCONNECTED' });
  assert.deepEqual(disconnected.moveHistory, pending.moveHistory);
  assert.equal(disconnected.currentSide, 'b');
  assert.equal(reduce(disconnected, { type: 'ENGINE_MOVE', revision: pending.revision, move: 'b9c7' }), disconnected);
  const resumed = inspect(reduce(disconnected, { type: 'SYNC', revision: pending.revision }), ['b9c7']);
  assert.equal(resumed.status, 'thinking');
  assert.deepEqual(engine(resumed, 'b9c7').moveHistory, ['b0c2', 'b9c7']);
});

test('0000 with legal moves is an error, while an empty legal set ends the game', () => {
  const pending = inspect(human(started(), 'b0c2'), ['b9c7']);
  const invalid = reduce(pending, { type: 'ENGINE_MOVE', revision: pending.revision, move: '0000' });
  assert.equal(invalid.status, 'error');
  assert.equal(invalid.phase, 'playing');
  assert.deepEqual(invalid.moveHistory, ['b0c2']);
  const ended = inspect(pending, [], true);
  assert.equal(ended.phase, 'ended');
  assert.equal(ended.currentSide, 'b');
  assert.equal(reduce(ended, { type: 'UNDO' }).phase, 'playing');
});

test('authoritative draws and wins end the game even when legal moves remain', () => {
  for (const gameStatus of ['draw', 'win', 'loss']) {
    const state = inspect(started(), ['a0a1'], false, gameStatus);
    assert.equal(state.phase, 'ended');
    assert.equal(state.status, 'idle');
    assert.equal(state.gameStatus, gameStatus);
    assert.equal(reduce(state, { type: 'PLAYER_MOVE', move: 'a0a1' }), state);
    assert.equal(reduce(state, { type: 'NEW_GAME' }).gameStatus, 'ongoing');
  }
});
