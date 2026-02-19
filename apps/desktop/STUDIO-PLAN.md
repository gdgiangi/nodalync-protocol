# Nodalync Studio — Interface Rebuild Plan

## Vision
Mirror the CLI visually. The desktop app IS the CLI but with a GUI. Every CLI command has a visual counterpart. Knowledge discovery, provenance tracking, and content management — not fancy 3D eye candy.

## CLI Commands → UI Mapping

### Phase 1: Core Knowledge Interface (Tasks 1-4)
The essential workflow: see your knowledge, find things, understand provenance.

**Task 1: Content Library View**
- Mirror: `nodalync list`, `nodalync search`
- UI: Sortable/filterable table of all local content (L0 sources, L2 entities, L3 syntheses)
- Columns: Title, Type (L0/L2/L3), Visibility, Price, Created, Sources count
- Search bar with real-time filtering
- Click → detail panel
- Acceptance: All content from `get_graph_data` rendered in table. Search filters work. Sorts work.

**Task 2: Content Detail & Provenance Panel**
- Mirror: `nodalync preview`, `nodalync versions`, entity detail
- UI: Slide-out panel showing full content metadata, version history, provenance chain
- Provenance: Visual tree showing L0→L1→L2→L3 derivation chain
- Who created it, when, what it derived from, what derived FROM it
- Acceptance: Click any content → see full provenance chain. Version history navigable.

**Task 3: Entity Graph (2D, Functional)**
- Mirror: `nodalync build-l2`, entity relationships
- UI: Clean 2D force-directed graph (NOT 3D). Nodes = entities, edges = relationships.
- Sidebar shows entity details on click. Edge labels on hover.
- Entity types distinguished by subtle icon/shape, NOT rainbow colors. Consistent amber/gold palette.
- Acceptance: All 234 entities visible. Click node → detail panel. Relationships readable. Zoom/pan works.

**Task 4: Search & Discovery**
- Mirror: `nodalync search`, `nodalync preview`
- UI: Global search bar (Ctrl+K or /) that searches across content AND entities
- Results grouped by type (Content, Entities, Relationships)
- Preview cards showing title, type, relevance score, provenance depth
- Acceptance: Search returns results within 200ms. Results link to detail views.

### Phase 2: Economics & Network (Tasks 5-7)

**Task 5: Balance & Earnings Dashboard**
- Mirror: `nodalync balance`, `nodalync earnings`, `nodalync deposit/withdraw`
- UI: Dashboard showing balance, earnings by content, transaction history
- Charts: earnings over time, top-earning content
- Actions: deposit, withdraw buttons
- Acceptance: Balance displays correctly. Earnings breakdown matches CLI output. Transaction history paginates.

**Task 6: Publishing Flow**
- Mirror: `nodalync publish`, `nodalync update`, `nodalync visibility`, `nodalync delete`
- UI: Drag-drop or file picker to publish. Edit metadata. Change visibility. Delete with confirmation.
- Publish wizard: select file → set title/description → set price → set visibility → confirm
- Acceptance: Can publish a file, update it, change visibility, delete it. All states reflected in content library.

**Task 7: Synthesis Creator**
- Mirror: `nodalync synthesize`, `nodalync build-l2`, `nodalync merge-l2`, `nodalync reference`
- UI: Select multiple sources → create L3 synthesis. Build L2 from L1s. Merge L2 graphs.
- Visual flow: source selection → synthesis config → output preview → publish option
- Acceptance: Can create L3 from multiple L0/L2 sources. Provenance chain correctly shows derivation.

### Phase 3: Network & Node (Tasks 8-9)

**Task 8: Node Status & Control**
- Mirror: `nodalync start`, `nodalync stop`, `nodalync status`
- UI: Status bar showing node state (online/offline), peer count, uptime
- Start/stop controls. Health indicators. Connected peers list.
- Acceptance: Start/stop node from UI. Status updates in real-time.

**Task 9: Payment Channels**
- Mirror: `nodalync open-channel`, `nodalync close-channel`, `nodalync list-channels`
- UI: Channel list with states, balances, peer info. Open/close actions.
- Acceptance: Can open channel, see balance, close channel from UI.

## Architecture Notes
- Each task = a React component/view that calls existing Tauri commands
- Tauri backend already has most commands wired up (graph_commands, publish_commands, fee_commands, etc.)
- Keep the 3D graph as an optional "visualization" mode, not the primary interface
- Primary interface = functional panels: Library, Detail, Search, Economics, Publish, Synthesize
- Navigation: Left sidebar with sections matching the phases above

## Ralph Wiggum Loop
Each task runs in a bash loop:
1. Agent reads the task spec + acceptance criteria
2. Agent implements the component
3. Agent runs `npm run build` to verify compilation
4. If build fails → fix and retry
5. If build passes → check acceptance criteria against the code
6. If criteria not met → iterate
7. If criteria met → move to next task
