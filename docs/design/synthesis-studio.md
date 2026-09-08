# Synthesis Studio

An editable design study for turning a body of knowledge into a new idea. The companion [SVG design board](synthesis-studio.svg) is 1440 × 1000. It is also available in [Figma: Nodalync — Synthesis Studio](https://www.figma.com/design/UDPKK1YLFVVXo4FX1azrw7/Nodalync-%E2%80%94-Synthesis-Studio?node-id=3-2). Its example text is fictional and does not represent user content or generated findings.

The native worktable implements the core gathering, thinking, writing, and provenance flow. The static board also illustrates future concepts; the implementation notes below distinguish them.

## What the interface is for

The primary outcome is a thought the user can develop, support, revise, and save. The main surface is a stable worktable of excerpts and observations beside a writing sheet. The current source picker searches original library content; a graph does not determine the arrangement of the user's thinking.

The design separates three kinds of material through labels, shapes, and restrained color:

| Material | Presentation | Meaning |
| --- | --- | --- |
| Source | Dark card with a numbered document tab | An excerpt with an immutable reference to its origin |
| Question | Pale amber annotation | Something the user wants to understand, including uncertainty or a tension |
| Idea | Pale green annotation | The user's interpretation or proposal, still open to revision |

The native app provides source passages and thought, question, and tension cards. Annotation leaders in the design board are a future concept, not a current interaction. Proximity, a shared source, or an extracted entity is not proof of a causal or semantic relationship. Source text and the user's writing stay visibly distinguishable.

## The working journey

1. **Frame the inquiry.** Start a named workspace with a question, or bring selected notes into a new workspace.
2. **Gather selectively.** Search the library in the source drawer. Read a document and bring an excerpt onto the worktable with its title and source reference.
3. **Juxtapose and think.** Arrange material side by side and add thought, question, or tension cards. An outline provides an alternative to the canvas. Existing card positions are preserved.
4. **Develop the draft.** Keep the writing sheet visible. Insert a source reference into the draft without losing the distinction between a quotation and an interpretation.
5. **Check the evidence.** Open a source from its card or draft reference, then return to the same workspace position.
6. **Resume or save.** Save the local draft and arrangement, or save a private derived idea with references to its original sources. The working materials remain available.

## Implemented native workflow

- Each board stores a title, working question, draft body, source passages, thinking cards, card positions, and viewport in the active profile's local storage.
- Original-source search is paginated in 12-row pages, with a native result cap of 40. Full source text is opened when needed. Search was tested with a fixture containing 10,001 manifests; this is a retrieval check, not a claim that the canvas renders that many cards.
- A board is bounded to 12 sources and 24 thinking cards; the local workspace supports up to 30 boards. These limits keep the visible working set separate from library size.
- Saving an idea creates private L3 derived content from the user's writing and 2–12 original source hashes. Its sources remain retrievable through the native provenance path.
- There is no AI provider or generation step. The app stores and derives the user's work; it does not synthesize prose on the user's behalf.

## Visual direction

The worktable uses a deep green charcoal, while the draft has a warm paper tone and an editorial reading width. Numbered source references provide continuity across the two surfaces. Text remains the main visual material. Card dimensions are substantial enough to compare passages at normal zoom; there is no orbiting camera, continuous physics, decorative particle field, or automatic global rearrangement.

The board shows an example composition, not a mandatory workflow or a fixed set of columns. Questions and ideas can emerge wherever the user places them.

## Future design concepts

The following are proposals and are **not implemented** in the current native worktable:

- Grouping cards into named, collapsible stacks, including the board's “Keep for later” stack.
- User-authored relationship leaders between cards.
- Collapsing the draft sheet or source drawer to expand the worktable.
- Semantic zoom that replaces paragraphs with stack titles and counts at overview scale.

These features should preserve stable positions and source references. They should not require users to formalize a relationship before they can compare two pieces of material.

## Scaling and accessibility

- Keep each board scoped to an inquiry and retain the implemented working-set limits. Library size must not determine canvas density.
- Use source search, board titles, and the outline to navigate. Zoom is a supporting control, not the only way to find material.
- Keep result counts and limits visible, and source labels readable at ordinary working zoom.
- Review keyboard actions for adding, selecting, moving, opening, and removing cards, with visible focus. Removing a card should remove its workspace reference, not the underlying library document.
- Review text labels alongside color, reduced motion, and nested interactive elements. These remain accessibility criteria to check, not a claim of complete conformance.

React Flow is a suitable foundation for custom document cards and a user-arranged worktable. Its built-in accessibility and viewport behavior are useful starting points, not a guarantee that the completed app is accessible. The viewport does not solve database retrieval or working-set size: both require explicit limits and validation.

## Implementation boundary

Gathering, comparison, questions, writing, and source references work locally. If an AI synthesis backend is added later, suggestions should remain reviewable, retain source attribution, and enter the workspace as suggestions rather than silently replacing the user's draft.

The SVG is an editable vector artifact authored in code, then imported into a new Figma draft through the authenticated browser's SVG clipboard workflow. The imported frame was renamed, its 1440 × 1000 dimensions and separate layers were checked, and the full composition was visually verified. Figma renders some font fallbacks differently; the local SVG preserves the intended serif reading typography. No existing Figma files were edited and no sharing settings were changed.

Blender was not installed in the inspected environment and was not used. No external images or font downloads are required for the SVG. Blender assets are unnecessary for the core reading and writing interaction.

## Validation and remaining review

Native manual testing exercised two original sources, a selected 173-character passage, a tension card, draft save and reopen, citations, the original reader and return path, and restart persistence of the draft and card positions. At this documentation update, 47 native tests and 14 frontend tests had passed; final branch validation is recorded separately.

Further review should cover comparing three passages, an empty library, a missing source, a long excerpt, a large library with a small board, keyboard-only navigation, and reduced motion. The checks above do not establish that every proposed interaction or accessibility criterion has passed.

## Primary references

- [VIKI: Spatial Hypertext Supporting Emergent Structure — Marshall and Shipman](https://people.engr.tamu.edu/shipman/abstracts/echt94-abstract.html). Supports using spatial arrangement to express partial and emerging structure. Applying that research to this worktable is a design judgment.
- [LiquidText features](https://www.liquidtext.net/features). A product precedent for keeping a workspace beside documents and returning from extracted material to original context. This study adds a persistent draft as its destination.
- [React Flow accessibility](https://reactflow.dev/learn/advanced-use/accessibility), [grouping](https://reactflow.dev/learn/layouting/sub-flows), and [performance](https://reactflow.dev/learn/advanced-use/performance). Implementation guidance for interactive cards, grouped workspaces, and bounded rendering.
- [W3C: Non-text Contrast](https://www.w3.org/WAI/WCAG21/understanding/non-text-contrast.html). Guidance for visible controls and meaningful graphic information.
