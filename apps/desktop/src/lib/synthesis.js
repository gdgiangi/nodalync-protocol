export const MAX_SOURCES = 12;
export const MAX_THOUGHTS = 24;
export const boardKey = (profileId) => `nodalync:synthesis:v1:${profileId}`;
export function blankBoard() {
  return { id: crypto.randomUUID(), title: "", question: "", body: "", sources: [], thoughts: [], viewport: { x: 24, y: 32, zoom: 1 }, saved: null, updatedAt: Date.now() };
}
export function citationFor(sources, hash) {
  const index = sources.findIndex((source) => source.hash === hash);
  return index < 0 ? null : `S${index + 1}`;
}
export function initialPassage(text, limit = 420) {
  // Skip a complete YAML header for the card preview, keeping exact offsets
  // into the untouched original for selection and citation.
  const frontmatter = text.match(/^\uFEFF?---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)(?:\r?\n|$)/);
  const headerEnd = frontmatter?.[0].length || 0;
  const whitespace = text.slice(headerEnd).match(/^\s*/)[0].length;
  const start = headerEnd + whitespace;
  const end = Math.min(text.length, start + limit);
  return { excerpt: text.slice(start, end), passage: { start, end } };
}
export function removeSource(board, hash) {
  const index = board.sources.findIndex((source) => source.hash === hash);
  if (index < 0) return board;
  if ([...board.body.matchAll(/\[S(\d+)\]/g)].some((match) => Number(match[1]) === index + 1)) throw new Error("This source is cited in your draft. Remove its citation before taking it off the board.");
  const sources = board.sources.filter((source) => source.hash !== hash);
  const body = board.body.replace(/\[S(\d+)\]/g, (match, number) => Number(number) > index + 1 ? `[S${Number(number) - 1}]` : match);
  return { ...board, sources, body };
}
export function citePassage(body, source, label) {
  const quote = source.excerpt.trim().split("\n").map((line) => `> ${line}`).join("\n");
  return `${body.trimEnd()}${body.trim() ? "\n\n" : ""}${quote}\n[${label}]\n\n`;
}
export function saveFingerprint(board) {
  return JSON.stringify([board.title.trim(), board.question.trim(), board.body, board.sources.map((source) => source.hash)]);
}
export function validateBoard(board) {
  if (!board.title.trim()) return "Give your new idea a title.";
  if (!board.question.trim()) return "Write a working question to guide the synthesis.";
  if (!board.body.trim()) return "Write the idea you want to save.";
  if (board.sources.length < 2) return "Bring in at least two original sources to make a synthesis.";
  if (board.sources.length > MAX_SOURCES) return `A working set can contain up to ${MAX_SOURCES} sources.`;
  const invalid = [...board.body.matchAll(/\[S(\d+)\]/g)].some((match) => match[1] !== String(Number(match[1])) || Number(match[1]) < 1 || Number(match[1]) > board.sources.length);
  return invalid ? "One of your source citations has no matching source on this board." : null;
}
export function readBoardStore(storage, key) {
  const raw = storage.getItem(key);
  if (!raw) return { version: 1, activeId: null, boards: [] };
  const result = JSON.parse(raw);
  const record = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
  const id = (value) => typeof value === "string" && value.length > 0;
  const position = (value) => record(value) && Number.isFinite(value.x) && Number.isFinite(value.y);
  if (!record(result) || result.version !== 1 || !Array.isArray(result.boards)) throw new Error("This saved worktable uses an unsupported format.");
  const validBoard = (board) => {
    if (!record(board) || !id(board.id) || ![board.title, board.question, board.body].every((value) => typeof value === "string") || !Array.isArray(board.sources) || !Array.isArray(board.thoughts)) return false;
    if (board.viewport !== undefined && (!position(board.viewport) || !Number.isFinite(board.viewport.zoom) || board.viewport.zoom <= 0)) return false;
    if (board.sources.some((source) => !record(source) || !id(source.hash) || typeof source.title !== "string" || typeof source.excerpt !== "string" || !position(source.position))) return false;
    if (board.thoughts.some((thought) => !record(thought) || !id(thought.id) || typeof thought.kind !== "string" || typeof thought.text !== "string" || !position(thought.position))) return false;
    const nodeIds = [...board.sources.map((source) => source.hash), ...board.thoughts.map((thought) => thought.id)];
    if (new Set(nodeIds).size !== nodeIds.length) return false;
    if (board.saved != null && (!record(board.saved) || !id(board.saved.hash) || ![board.saved.title, board.saved.text, board.saved.fingerprint].every((value) => typeof value === "string"))) return false;
    return true;
  };
  if (!result.boards.every(validBoard) || new Set(result.boards.map((board) => board.id)).size !== result.boards.length) throw new Error("The saved worktable could not be read. Your stored data has been kept.");
  return result;
}
