/**
 * Shared constants for Nodalync Studio desktop app.
 * Single source of truth for entity type colors, predicates, and config.
 */

// ═══ Entity Type Colors ═══
// Vibrant but tasteful — each type gets a distinct hue for graph readability
export const TYPE_COLORS = {
  Person: "#e599f7",
  Organization: "#74c0fc",
  Concept: "#69db7c",
  Decision: "#ffd43b",
  Task: "#ff8787",
  Asset: "#a9e34b",
  Goal: "#f783ac",
  Pattern: "#66d9e8",
  Insight: "#b197fc",
  Value: "#ffa94d",
  Technology: "#20c997",
  Event: "#87ceeb",
  Location: "#dda0dd",
  Product: "#98d8c8",
  Work: "#fff176",
  Metric: "#ff7043",
  TimePoint: "#ab47bc",
};

export const DEFAULT_COLOR = "#868e96";

export function getEntityColor(type) {
  return TYPE_COLORS[type] || DEFAULT_COLOR;
}

// ═══ Relationship Predicates ═══
// Human-readable labels for the fixed ontology predicates
export const PREDICATE_LABELS = {
  worksFor: "works for",
  workedFor: "worked for",
  locatedIn: "located in",
  basedIn: "based in",
  createdBy: "created by",
  authorOf: "author of",
  partOf: "part of",
  memberOf: "member of",
  relatedTo: "related to",
  mentions: "mentions",
  discusses: "discusses",
  before: "before",
  after: "after",
  during: "during",
  causes: "causes",
  enables: "enables",
  prevents: "prevents",
  isA: "is a",
  instanceOf: "instance of",
  hasProperty: "has property",
  hasAttribute: "has attribute",
  uses: "uses",
  usedBy: "used by",
  fundedBy: "funded by",
  investedIn: "invested in",
  acquiredBy: "acquired by",
};

export function formatPredicate(pred) {
  return PREDICATE_LABELS[pred] || pred;
}

// ═══ Content Types ═══
export const CONTENT_TYPES = [
  "journal",
  "note",
  "article",
  "research",
  "insight",
  "question",
  "answer",
  "documentation",
  "custom",
];

// ═══ Visibility Options ═══
export const VISIBILITY_OPTIONS = ["private", "unlisted", "shared"];

// ═══ Graph Visualization Config ═══
export const GRAPH_CONFIG = {
  BG_COLOR: "#06060a",
  LINK_COLOR: "rgba(255, 255, 255, 0.06)",
  LINK_HOVER_COLOR: "rgba(255, 255, 255, 0.12)",
  LABEL_COLOR: "rgba(255, 255, 255, 0.45)",
  LABEL_DIM_COLOR: "rgba(255, 255, 255, 0.2)",
  MIN_NODE_RADIUS: 4,
  MAX_NODE_RADIUS: 20,
  LINK_LABEL_THRESHOLD: 80, // hide link labels above this edge count
};

// ═══ App Version ═══
export const APP_VERSION = "0.1.0";
