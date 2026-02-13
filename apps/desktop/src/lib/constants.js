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

// ═══ Relationship Category Colors ═══
// Edges are tinted by semantic category for visual distinction
export const EDGE_CATEGORY_COLORS = {
  structural: "rgba(116, 192, 252, 0.35)",   // blue  — partOf, memberOf, instanceOf, isA
  causal:     "rgba(255, 135, 135, 0.35)",    // red   — causes, enables, prevents
  temporal:   "rgba(171, 71, 188, 0.35)",     // purple — before, after, during
  spatial:    "rgba(221, 160, 221, 0.35)",    // pink  — locatedIn, basedIn
  action:     "rgba(74, 222, 128, 0.35)",     // green — createdBy, authorOf, uses, usedBy
  financial:  "rgba(255, 169, 77, 0.35)",     // orange — fundedBy, investedIn, acquiredBy
  reference:  "rgba(255, 255, 255, 0.18)",    // white dim — mentions, discusses, relatedTo
};

const PREDICATE_TO_CATEGORY = {
  partOf: "structural", memberOf: "structural", instanceOf: "structural", isA: "structural",
  causes: "causal", enables: "causal", prevents: "causal",
  before: "temporal", after: "temporal", during: "temporal",
  locatedIn: "spatial", basedIn: "spatial",
  createdBy: "action", authorOf: "action", uses: "action", usedBy: "action",
  worksFor: "action", workedFor: "action",
  fundedBy: "financial", investedIn: "financial", acquiredBy: "financial",
  mentions: "reference", discusses: "reference", relatedTo: "reference",
};

export function getEdgeColor(predicate) {
  const cat = PREDICATE_TO_CATEGORY[predicate] || "reference";
  return EDGE_CATEGORY_COLORS[cat];
}

export function getEdgeHighlightColor(predicate) {
  const cat = PREDICATE_TO_CATEGORY[predicate] || "reference";
  // Brighten the category color for hover
  return EDGE_CATEGORY_COLORS[cat].replace(/[\d.]+\)$/, "0.7)");
}

// ═══ Graph Visualization Config ═══
export const GRAPH_CONFIG = {
  BG_COLOR: "#06060a",
  LINK_COLOR: "rgba(255, 255, 255, 0.18)",
  LINK_HOVER_COLOR: "rgba(255, 255, 255, 0.45)",
  LINK_DIM_COLOR: "rgba(255, 255, 255, 0.06)",
  LABEL_COLOR: "rgba(255, 255, 255, 0.45)",
  LABEL_DIM_COLOR: "rgba(255, 255, 255, 0.2)",
  MIN_NODE_RADIUS: 4,
  MAX_NODE_RADIUS: 20,
  MIN_LINK_WIDTH: 1,
  MAX_LINK_WIDTH: 3,
  LINK_LABEL_THRESHOLD: 80, // hide link labels above this edge count
};

// ═══ App Version ═══
export const APP_VERSION = "0.1.0";
