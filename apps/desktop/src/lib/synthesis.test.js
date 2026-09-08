import test from 'node:test';
import assert from 'node:assert/strict';
import { blankBoard, removeSource, citePassage, validateBoard, saveFingerprint, readBoardStore } from './synthesis.js';
const sources = [{ hash: 'a', excerpt: 'One source\nTwo lines' }, { hash: 'b', excerpt: 'A competing account' }, { hash: 'c', excerpt: 'Another view' }];
const board = { id: 'draft', title: 'New idea', question: 'What if?', body: 'My interpretation [S2] and [S3].', sources, thoughts: [] };
const storedBoard = {
  ...board,
  viewport: { x: 24, y: 32, zoom: 0.85 },
  sources: sources.map((source, index) => ({ ...source, title: `Source ${index + 1}`, position: { x: index * 300, y: 30 } })),
  thoughts: [{ id: 'thought', kind: 'question', text: 'What connects these?', position: { x: 650, y: 40 } }],
  saved: { hash: 'saved-document', title: board.title, text: '# New idea', fingerprint: saveFingerprint(board) },
};
test('removing an unused source keeps every remaining citation tied to the same original', () => {
  const result = removeSource(board, 'a');
  assert.deepEqual(result.sources.map((source) => source.hash), ['b', 'c']);
  assert.equal(result.body, 'My interpretation [S1] and [S2].');
  assert.equal(board.body, 'My interpretation [S2] and [S3].');
  assert.throws(() => removeSource(board, 'b'), /cited/);
});
test('citing an excerpt preserves multiline text and identifies its actual source', () => {
  assert.equal(citePassage('A new idea.', sources[0], 'S1'), 'A new idea.\n\n> One source\n> Two lines\n[S1]\n\n');
});
test('a synthesis needs real foundations and no dangling citation labels', () => {
  assert.equal(validateBoard(board), null);
  assert.match(validateBoard({ ...board, sources: sources.slice(0, 1) }), /at least two/);
  assert.match(validateBoard({ ...board, body: 'Unsupported [S4]' }), /no matching source/);
  assert.match(validateBoard({ ...board, body: 'Unsupported [S0]' }), /no matching source/);
  assert.match(validateBoard({ ...board, title: '  ' }), /title/);
});
test('whitespace edits change the saved document fingerprint; moving a card does not', () => {
  assert.notEqual(saveFingerprint(board), saveFingerprint({ ...board, body: board.body + '\n' }));
  assert.equal(saveFingerprint(board), saveFingerprint({ ...board, sources: sources.map((source) => ({ ...source, position: { x: 30, y: 20 } })) }));
});
test('unsupported or damaged draft data is rejected without overwriting storage', () => {
  const raw = JSON.stringify({ version: 7, boards: [] });
  const storage = { getItem: () => raw, setItem: () => assert.fail('must not overwrite') };
  assert.throws(() => readBoardStore(storage, 'profile'), /unsupported/);
  const saved = { version: 1, activeId: 'draft', boards: [storedBoard, blankBoard()] };
  assert.deepEqual(readBoardStore({ getItem: () => JSON.stringify(saved) }, 'profile'), saved);
});

test('malformed render fields fail restoration while preserving the exact stored draft', () => {
  const corruptions = [
    (draft) => { draft.title = null; },
    (draft) => { draft.question = {}; },
    (draft) => { draft.sources[0] = null; },
    (draft) => { delete draft.sources[0].excerpt; },
    (draft) => { draft.sources[0].title = {}; },
    (draft) => { draft.sources[0].position.x = '30'; },
    (draft) => { draft.thoughts[0].text = []; },
    (draft) => { draft.thoughts[0].position = null; },
    (draft) => { draft.thoughts[0].id = draft.sources[0].hash; },
    (draft) => { draft.viewport.zoom = 0; },
    (draft) => { draft.saved.title = {}; },
  ];
  for (const corrupt of corruptions) {
    const draft = structuredClone(storedBoard);
    corrupt(draft);
    const raw = JSON.stringify({ version: 1, activeId: draft.id, boards: [draft] });
    let persisted = raw;
    const storage = { getItem: () => persisted, setItem: (_, value) => { persisted = value; assert.fail('must not overwrite a damaged draft'); } };
    assert.throws(() => readBoardStore(storage, 'profile'), /stored data has been kept/);
    assert.equal(persisted, raw);
  }
});

test("noncanonical citations cannot be silently rebound by removing a source", () => {
  const noncanonical = { ...board, body: "An idea grounded in [S01]" };
  assert.match(validateBoard(noncanonical), /no matching source/);
  assert.throws(() => removeSource(noncanonical, "a"), /cited/);
});
