import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./studio-content.css";

function errorMessage(error) {
  return typeof error === "string" ? error : error?.message || "Something went wrong. Please try again.";
}

export default function CreateContentDialog({ isOpen, onClose, onCreated }) {
  const [title, setTitle] = useState("");
  const [text, setText] = useState("");
  const [description, setDescription] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(null);
  const dialogRef = useRef(null);
  const formRef = useRef(null);
  const titleRef = useRef(null);
  const textRef = useRef(null);
  const submittingRef = useRef(false);
  const previousFocusRef = useRef(null);
  const savedRef = useRef(null);

  useEffect(() => {
    if (!isOpen) return;
    previousFocusRef.current = document.activeElement;
    setTitle("");
    setText("");
    setDescription("");
    setError("");
    setSaved(null);
    const dialog = dialogRef.current;
    if (dialog && !dialog.open) dialog.showModal();
    titleRef.current?.focus();
    return () => {
      dialog?.close();
      previousFocusRef.current?.focus?.();
    };
  }, [isOpen]);

  useEffect(() => {
    if (saved) savedRef.current?.focus();
  }, [saved]);

  function closeDialog() {
    if (!submittingRef.current) onClose?.();
  }

  async function indexContent(result) {
    try {
      const extraction = await invoke("extract_mentions", { contentHash: result.hash });
      return { ...result, extraction, extractionError: null };
    } catch (indexError) {
      return { ...result, extractionError: errorMessage(indexError) };
    }
  }

  async function submit(event) {
    event.preventDefault();
    if (submittingRef.current || saved) return;
    if (!title.trim() || !text.trim()) {
      setError(!title.trim() ? "Give your note a title." : "Add some content to your note.");
      (!title.trim() ? titleRef : textRef).current?.focus();
      return;
    }
    submittingRef.current = true;
    setSubmitting(true);
    setError("");
    try {
      const result = await invoke("add_text_content", {
        text,
        title: title.trim(),
        description: description.trim() || null,
      });
      const indexed = await indexContent(result);
      setSaved(indexed);
      onCreated?.(indexed);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }

  async function retryIndexing() {
    if (submittingRef.current || !saved) return;
    submittingRef.current = true;
    setSubmitting(true);
    try {
      const indexed = await indexContent(saved);
      setSaved(indexed);
      onCreated?.(indexed);
    } finally {
      submittingRef.current = false;
      setSubmitting(false);
    }
  }

  if (!isOpen) return null;
  const words = text.trim() ? text.trim().split(/\s+/).length : 0;

  return (
    <dialog
      ref={dialogRef}
      className="sc-dialog"
      aria-labelledby="sc-note-heading"
      aria-describedby="sc-note-description"
      onCancel={(event) => { event.preventDefault(); closeDialog(); }}
      onClick={(event) => {
        if (event.target !== event.currentTarget) return;
        const bounds = event.currentTarget.getBoundingClientRect();
        if (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom) closeDialog();
      }}
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
          event.preventDefault();
          formRef.current?.requestSubmit();
        }
      }}
    >
      <form ref={formRef} onSubmit={submit} className="sc-note-form">
        <header className="sc-dialog-header">
          <div>
            <span className="sc-eyebrow">Your originals</span>
            <h2 id="sc-note-heading">{saved ? "Note saved" : "Write a note"}</h2>
            <p id="sc-note-description">Capture an idea. Keep its source.</p>
          </div>
          <button type="button" className="sc-icon-button" onClick={closeDialog} disabled={submitting} aria-label="Close note editor">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg>
          </button>
        </header>

        {saved ? (
          <div className="sc-save-result" ref={savedRef} tabIndex={-1}>
            <div className="sc-success-icon" aria-hidden="true">
              <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8"><path d="m5 12 4 4L19 6" /></svg>
            </div>
            <h3>{saved.title}</h3>
            <p>Your note is saved privately in your library.</p>
            {saved.extractionError ? (
              <div className="sc-message sc-message-warning" role="status">
                <strong>Saved, but graph indexing needs attention.</strong>
                <span>{saved.extractionError}</span>
                <button type="button" className="sc-button" onClick={retryIndexing} disabled={submitting}>{submitting ? "Indexing…" : "Retry indexing"}</button>
              </div>
            ) : (
              <p className="sc-muted">{saved.extraction?.mention_count ?? saved.mentions ?? 0} mentions · {saved.extraction?.entities?.length || 0} connected entities</p>
            )}
          </div>
        ) : (
          <div className="sc-dialog-body">
            <label className="sc-field">
              <span>Title</span>
              <input ref={titleRef} value={title} onChange={(event) => setTitle(event.target.value)} placeholder="What would you like to remember?" maxLength={200} required disabled={submitting} autoComplete="off" />
            </label>
            <label className="sc-field">
              <span>Note</span>
              <textarea ref={textRef} value={text} onChange={(event) => setText(event.target.value)} placeholder="Write an observation, an idea, or something you learned…" required disabled={submitting} />
              <span className="sc-field-hint">{words} {words === 1 ? "word" : "words"}</span>
            </label>
            <label className="sc-field">
              <span>Description <span className="sc-optional">optional</span></span>
              <input value={description} onChange={(event) => setDescription(event.target.value)} placeholder="A little context for later" maxLength={500} disabled={submitting} />
            </label>
            {error && <div className="sc-message sc-message-error" role="alert">{error}</div>}
          </div>
        )}

        <footer className="sc-dialog-footer">
          <span className="sc-privacy-note">
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" aria-hidden="true"><rect x="5" y="10" width="14" height="11" rx="2" /><path d="M8 10V7a4 4 0 0 1 8 0v3" /></svg>
            Private · stored on this node
          </span>
          <div className="sc-actions">
            <button type="button" className="sc-button" onClick={closeDialog} disabled={submitting}>{saved ? "Done" : "Cancel"}</button>
            {!saved && <button type="submit" className="sc-button sc-button-primary" disabled={submitting}>{submitting && <span className="sc-spinner" />}{submitting ? "Saving note…" : "Save note"}</button>}
          </div>
        </footer>
      </form>
    </dialog>
  );
}
