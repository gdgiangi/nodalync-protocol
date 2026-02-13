import { useState, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

const CONTENT_TYPES = [
  { value: "journal", label: "Journal", icon: "📓", desc: "Daily thoughts & reflections" },
  { value: "note", label: "Note", icon: "📝", desc: "Quick captures & snippets" },
  { value: "article", label: "Article", icon: "📄", desc: "Long-form writing" },
  { value: "research", label: "Research", icon: "🔬", desc: "Findings & analysis" },
  { value: "insight", label: "Insight", icon: "💡", desc: "Observations & patterns" },
  { value: "question", label: "Question", icon: "❓", desc: "Things to explore" },
  { value: "answer", label: "Answer", icon: "✅", desc: "Responses & solutions" },
  { value: "documentation", label: "Docs", icon: "📚", desc: "Technical documentation" },
];

const VISIBILITY_OPTIONS = [
  { value: "private", label: "Private", icon: "🔒", desc: "Only you" },
  { value: "unlisted", label: "Unlisted", icon: "🔗", desc: "Anyone with the link" },
  { value: "shared", label: "Shared", icon: "🌐", desc: "Published to network" },
];

export default function CreateContentDialog({ isOpen, onClose, onCreated }) {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [contentType, setContentType] = useState("note");
  const [visibility, setVisibility] = useState("private");
  const [tags, setTags] = useState([]);
  const [tagInput, setTagInput] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(false);
  const [showTypeSelector, setShowTypeSelector] = useState(false);
  const [animateIn, setAnimateIn] = useState(false);

  const titleRef = useRef(null);
  const contentRef = useRef(null);
  const typeSelectorRef = useRef(null);

  // Animate in
  useEffect(() => {
    if (isOpen) {
      requestAnimationFrame(() => setAnimateIn(true));
      // Focus title on open
      setTimeout(() => titleRef.current?.focus(), 100);
    } else {
      setAnimateIn(false);
    }
  }, [isOpen]);

  // Close type selector on outside click
  useEffect(() => {
    function handleClick(e) {
      if (typeSelectorRef.current && !typeSelectorRef.current.contains(e.target)) {
        setShowTypeSelector(false);
      }
    }
    if (showTypeSelector) {
      document.addEventListener("mousedown", handleClick);
      return () => document.removeEventListener("mousedown", handleClick);
    }
  }, [showTypeSelector]);

  // Reset form
  const resetForm = useCallback(() => {
    setTitle("");
    setContent("");
    setContentType("note");
    setVisibility("private");
    setTags([]);
    setTagInput("");
    setError(null);
    setSuccess(false);
  }, []);

  const handleClose = useCallback(() => {
    setAnimateIn(false);
    setTimeout(() => {
      onClose();
      resetForm();
    }, 250);
  }, [onClose, resetForm]);

  // Keyboard: Escape to close, Ctrl+Enter to submit
  useEffect(() => {
    if (!isOpen) return;
    function handleKeyDown(e) {
      if (e.key === "Escape") {
        handleClose();
        e.preventDefault();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
        handleSubmit();
        e.preventDefault();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, handleClose, title, content, contentType, visibility, tags]);

  // Auto-resize textarea
  const handleContentChange = (e) => {
    setContent(e.target.value);
    const textarea = e.target;
    textarea.style.height = "auto";
    textarea.style.height = Math.min(textarea.scrollHeight, 400) + "px";
  };

  // Tag management
  const handleTagKeyDown = (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addTag();
    }
    if (e.key === "Backspace" && !tagInput && tags.length > 0) {
      setTags(tags.slice(0, -1));
    }
  };

  const addTag = () => {
    const tag = tagInput.trim().toLowerCase().replace(/,/g, "");
    if (tag && !tags.includes(tag) && tags.length < 10) {
      setTags([...tags, tag]);
      setTagInput("");
    }
  };

  const removeTag = (tagToRemove) => {
    setTags(tags.filter((t) => t !== tagToRemove));
  };

  // Submit
  async function handleSubmit() {
    if (!title.trim()) {
      setError("Title is required");
      titleRef.current?.focus();
      return;
    }
    if (!content.trim()) {
      setError("Content is required");
      contentRef.current?.focus();
      return;
    }

    try {
      setSubmitting(true);
      setError(null);

      const result = await invoke("create_l0", {
        content: content.trim(),
        contentType: contentType,
        title: title.trim(),
        tags: tags.length > 0 ? tags : undefined,
        visibility: visibility,
      });

      setSuccess(true);
      onCreated?.(result);

      // Auto-close after success
      setTimeout(() => {
        handleClose();
      }, 1200);
    } catch (err) {
      console.error("Failed to create content:", err);
      setError(typeof err === "string" ? err : err?.message || "Failed to create content");
    } finally {
      setSubmitting(false);
    }
  }

  if (!isOpen) return null;

  const selectedType = CONTENT_TYPES.find((t) => t.value === contentType);
  const selectedVisibility = VISIBILITY_OPTIONS.find((v) => v.value === visibility);
  const charCount = content.length;
  const wordCount = content.trim() ? content.trim().split(/\s+/).length : 0;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-50"
        style={{
          background: animateIn ? "rgba(0, 0, 0, 0.5)" : "transparent",
          backdropFilter: animateIn ? "blur(4px)" : "none",
          transition: "all 0.25s ease",
        }}
        onClick={handleClose}
      />

      {/* Dialog */}
      <div
        className="fixed inset-0 z-50 flex items-center justify-center p-6"
        style={{ pointerEvents: "none" }}
      >
        <div
          className="w-full max-w-2xl flex flex-col"
          style={{
            pointerEvents: "auto",
            maxHeight: "85vh",
            background: "rgba(10, 10, 16, 0.95)",
            backdropFilter: "blur(32px)",
            WebkitBackdropFilter: "blur(32px)",
            border: "1px solid var(--border-subtle)",
            borderRadius: "var(--radius-lg)",
            boxShadow: "0 24px 80px rgba(0, 0, 0, 0.5), 0 0 1px rgba(255, 255, 255, 0.05)",
            opacity: animateIn ? 1 : 0,
            transform: animateIn ? "translateY(0) scale(1)" : "translateY(12px) scale(0.98)",
            transition: "all 0.3s cubic-bezier(0.4, 0, 0.2, 1)",
          }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* Header */}
          <div
            className="flex items-center justify-between px-5 py-3.5 flex-shrink-0"
            style={{ borderBottom: "1px solid var(--border-subtle)" }}
          >
            <div className="flex items-center gap-3">
              <div
                className="w-7 h-7 rounded-md flex items-center justify-center"
                style={{
                  background: "var(--accent-dim)",
                  border: "1px solid rgba(92, 124, 250, 0.2)",
                }}
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{ color: "var(--accent)" }}
                >
                  <line x1="12" y1="5" x2="12" y2="19" />
                  <line x1="5" y1="12" x2="19" y2="12" />
                </svg>
              </div>
              <h2
                className="text-[13px] font-medium"
                style={{ color: "var(--text-primary)", letterSpacing: "0.3px" }}
              >
                New Content
              </h2>
            </div>

            <div className="flex items-center gap-2">
              <kbd
                className="mono text-[9px] px-1.5 py-0.5 rounded hidden sm:inline-block"
                style={{
                  color: "var(--text-ghost)",
                  background: "rgba(255, 255, 255, 0.03)",
                  border: "1px solid rgba(255, 255, 255, 0.06)",
                }}
              >
                ESC
              </kbd>
              <button
                onClick={handleClose}
                className="w-7 h-7 flex items-center justify-center rounded-md"
                style={{
                  color: "var(--text-ghost)",
                  transition: "all 0.15s ease",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--bg-hover)";
                  e.currentTarget.style.color = "var(--text-secondary)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "transparent";
                  e.currentTarget.style.color = "var(--text-ghost)";
                }}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          </div>

          {/* Content type + visibility row */}
          <div
            className="flex items-center gap-3 px-5 py-2.5 flex-shrink-0"
            style={{ borderBottom: "1px solid var(--border-subtle)" }}
          >
            {/* Content type selector */}
            <div className="relative" ref={typeSelectorRef}>
              <button
                onClick={() => setShowTypeSelector(!showTypeSelector)}
                className="flex items-center gap-2 px-2.5 py-1.5 rounded-md"
                style={{
                  background: showTypeSelector ? "var(--bg-hover)" : "var(--bg-surface)",
                  border: "1px solid var(--border-subtle)",
                  cursor: "pointer",
                  transition: "all 0.15s ease",
                }}
                onMouseEnter={(e) => {
                  if (!showTypeSelector) e.currentTarget.style.background = "var(--bg-elevated)";
                }}
                onMouseLeave={(e) => {
                  if (!showTypeSelector) e.currentTarget.style.background = "var(--bg-surface)";
                }}
              >
                <span className="text-[13px]">{selectedType?.icon}</span>
                <span className="text-[11px]" style={{ color: "var(--text-secondary)" }}>
                  {selectedType?.label}
                </span>
                <svg
                  width="10"
                  height="10"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  style={{
                    color: "var(--text-ghost)",
                    transform: showTypeSelector ? "rotate(180deg)" : "rotate(0)",
                    transition: "transform 0.2s ease",
                  }}
                >
                  <polyline points="6 9 12 15 18 9" />
                </svg>
              </button>

              {/* Dropdown */}
              {showTypeSelector && (
                <div
                  className="absolute top-full left-0 mt-1 py-1 rounded-lg z-10"
                  style={{
                    background: "rgba(12, 12, 20, 0.98)",
                    border: "1px solid var(--border-default)",
                    boxShadow: "0 8px 32px rgba(0, 0, 0, 0.4)",
                    minWidth: 220,
                    animation: "scale-in 0.15s ease",
                  }}
                >
                  {CONTENT_TYPES.map((type) => (
                    <button
                      key={type.value}
                      onClick={() => {
                        setContentType(type.value);
                        setShowTypeSelector(false);
                      }}
                      className="w-full flex items-center gap-3 px-3 py-2 text-left"
                      style={{
                        background: contentType === type.value ? "var(--accent-dim)" : "transparent",
                        border: "none",
                        cursor: "pointer",
                        transition: "background 0.1s ease",
                      }}
                      onMouseEnter={(e) => {
                        if (contentType !== type.value) e.currentTarget.style.background = "var(--bg-hover)";
                      }}
                      onMouseLeave={(e) => {
                        if (contentType !== type.value) e.currentTarget.style.background = "transparent";
                      }}
                    >
                      <span className="text-[14px] flex-shrink-0">{type.icon}</span>
                      <div>
                        <div
                          className="text-[11px]"
                          style={{
                            color: contentType === type.value ? "var(--accent)" : "var(--text-primary)",
                          }}
                        >
                          {type.label}
                        </div>
                        <div className="text-[9px]" style={{ color: "var(--text-ghost)" }}>
                          {type.desc}
                        </div>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Visibility toggle */}
            <div className="flex items-center rounded-md overflow-hidden" style={{ border: "1px solid var(--border-subtle)" }}>
              {VISIBILITY_OPTIONS.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => setVisibility(opt.value)}
                  className="flex items-center gap-1.5 px-2.5 py-1.5"
                  style={{
                    background: visibility === opt.value ? "var(--bg-active)" : "transparent",
                    border: "none",
                    cursor: "pointer",
                    transition: "all 0.15s ease",
                    borderRight: opt.value !== "shared" ? "1px solid var(--border-subtle)" : "none",
                  }}
                  onMouseEnter={(e) => {
                    if (visibility !== opt.value) e.currentTarget.style.background = "var(--bg-hover)";
                  }}
                  onMouseLeave={(e) => {
                    if (visibility !== opt.value) e.currentTarget.style.background = "transparent";
                  }}
                  title={opt.desc}
                >
                  <span className="text-[11px]">{opt.icon}</span>
                  <span
                    className="text-[10px]"
                    style={{
                      color: visibility === opt.value ? "var(--text-primary)" : "var(--text-ghost)",
                    }}
                  >
                    {opt.label}
                  </span>
                </button>
              ))}
            </div>

            {/* Word/char count */}
            <div className="ml-auto flex items-center gap-2">
              <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
                {wordCount} words
              </span>
              <span style={{ color: "rgba(255,255,255,0.06)" }}>·</span>
              <span className="mono text-[9px]" style={{ color: "var(--text-ghost)" }}>
                {charCount.toLocaleString()} chars
              </span>
            </div>
          </div>

          {/* Scrollable form body */}
          <div className="flex-1 overflow-y-auto px-5 py-4" style={{ minHeight: 0 }}>
            {/* Title */}
            <input
              ref={titleRef}
              type="text"
              value={title}
              onChange={(e) => {
                setTitle(e.target.value);
                if (error) setError(null);
              }}
              placeholder="Title"
              className="w-full bg-transparent outline-none text-[18px] font-light mb-4"
              style={{
                color: "var(--text-primary)",
                border: "none",
                letterSpacing: "-0.2px",
                lineHeight: 1.3,
              }}
              maxLength={200}
              autoComplete="off"
            />

            {/* Content */}
            <textarea
              ref={contentRef}
              value={content}
              onChange={(e) => {
                handleContentChange(e);
                if (error) setError(null);
              }}
              placeholder="Write your thoughts..."
              className="w-full bg-transparent outline-none resize-none text-[13px] leading-relaxed"
              style={{
                color: "var(--text-secondary)",
                border: "none",
                minHeight: 160,
                fontFamily: "'SF Pro Display', -apple-system, 'Segoe UI', sans-serif",
              }}
            />

            {/* Tags */}
            <div
              className="mt-4 pt-4"
              style={{ borderTop: "1px solid var(--border-subtle)" }}
            >
              <label className="label-xs block mb-2">TAGS</label>
              <div
                className="flex flex-wrap items-center gap-1.5 p-2 rounded-md min-h-[36px]"
                style={{
                  background: "rgba(255, 255, 255, 0.02)",
                  border: "1px solid var(--border-subtle)",
                }}
              >
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full animate-scale-in"
                    style={{
                      background: "var(--accent-dim)",
                      border: "1px solid rgba(92, 124, 250, 0.2)",
                      fontSize: 10,
                      color: "var(--accent)",
                    }}
                  >
                    {tag}
                    <button
                      onClick={() => removeTag(tag)}
                      className="flex items-center justify-center w-3 h-3 rounded-full hover:bg-[rgba(255,255,255,0.1)] transition-colors"
                      style={{
                        border: "none",
                        background: "transparent",
                        cursor: "pointer",
                        color: "inherit",
                        lineHeight: 1,
                      }}
                    >
                      ×
                    </button>
                  </span>
                ))}
                <input
                  type="text"
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={handleTagKeyDown}
                  onBlur={addTag}
                  placeholder={tags.length === 0 ? "Add tags (press Enter)" : tags.length < 10 ? "Add more..." : ""}
                  disabled={tags.length >= 10}
                  className="flex-1 bg-transparent outline-none text-[11px] min-w-[80px]"
                  style={{
                    color: "var(--text-secondary)",
                    border: "none",
                  }}
                />
              </div>
              {tags.length >= 10 && (
                <span className="text-[9px] mt-1 block" style={{ color: "var(--text-ghost)" }}>
                  Maximum 10 tags
                </span>
              )}
            </div>
          </div>

          {/* Footer */}
          <div
            className="flex items-center justify-between px-5 py-3 flex-shrink-0"
            style={{ borderTop: "1px solid var(--border-subtle)" }}
          >
            {/* Error message */}
            <div className="flex-1">
              {error && (
                <span
                  className="text-[11px] animate-fade-in"
                  style={{ color: "var(--red)" }}
                >
                  {error}
                </span>
              )}
              {success && (
                <span
                  className="text-[11px] animate-fade-in flex items-center gap-1.5"
                  style={{ color: "var(--green)" }}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                  Content created — AI processing started
                </span>
              )}
            </div>

            <div className="flex items-center gap-2">
              <button
                onClick={handleClose}
                className="btn text-[11px]"
                disabled={submitting}
              >
                Cancel
              </button>
              <button
                onClick={handleSubmit}
                disabled={submitting || success || !title.trim() || !content.trim()}
                className="btn text-[11px] relative overflow-hidden"
                style={{
                  background: submitting || !title.trim() || !content.trim()
                    ? "rgba(92, 124, 250, 0.05)"
                    : "rgba(92, 124, 250, 0.12)",
                  borderColor: submitting || !title.trim() || !content.trim()
                    ? "rgba(92, 124, 250, 0.1)"
                    : "rgba(92, 124, 250, 0.3)",
                  color: submitting || !title.trim() || !content.trim()
                    ? "rgba(92, 124, 250, 0.3)"
                    : "var(--accent)",
                  cursor: submitting || !title.trim() || !content.trim() ? "not-allowed" : "pointer",
                }}
              >
                {submitting ? (
                  <span className="flex items-center gap-2">
                    <div
                      className="w-3 h-3 rounded-full border-2"
                      style={{
                        borderColor: "rgba(92, 124, 250, 0.15)",
                        borderTopColor: "var(--accent)",
                        animation: "spin 0.8s linear infinite",
                      }}
                    />
                    Creating...
                  </span>
                ) : (
                  <span className="flex items-center gap-1.5">
                    Create
                    <kbd
                      className="mono text-[8px] px-1 py-px rounded"
                      style={{
                        background: "rgba(255, 255, 255, 0.05)",
                        border: "1px solid rgba(255, 255, 255, 0.08)",
                        opacity: !title.trim() || !content.trim() ? 0.3 : 0.6,
                      }}
                    >
                      ⌃↵
                    </kbd>
                  </span>
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
