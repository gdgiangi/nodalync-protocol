# Ralph Wiggum Loop for Nodalync Studio
# Usage: .\ralph-loop.ps1 -TaskNum 1
# Runs Claude Code in a loop until the task passes acceptance criteria

param(
    [int]$TaskNum = 1,
    [int]$MaxIterations = 20
)

$PLAN = Get-Content -Raw "$PSScriptRoot\STUDIO-PLAN.md"
$DESKTOP_DIR = $PSScriptRoot

$TASKS = @{
    1 = @{
        Name = "Content Library View"
        Prompt = @"
You are building Nodalync Studio, a desktop app (Tauri 2.0 + React).

TASK: Build the Content Library View component.

READ FIRST:
- $DESKTOP_DIR\STUDIO-PLAN.md (full plan)
- $DESKTOP_DIR\src\App.jsx (current app structure)
- $DESKTOP_DIR\rust-src\graph_commands.rs (Tauri backend commands available)
- $DESKTOP_DIR\src\components\ (existing components)

WHAT TO BUILD:
- A sortable/filterable table view of ALL local content (L0 sources, L2 entities, L3 syntheses)
- Columns: Title/Label, Type (L0/L2/L3), Entity Type, Visibility, Source Count, Created
- Search bar with real-time filtering across title and entity_type
- Click any row → opens detail panel (EntityDetailPanel already exists)
- Tab/toggle to switch between "Table View" and "Graph View" (existing graph)
- This should be the DEFAULT view when the app opens (graph is secondary)

USE EXISTING:
- Tauri commands: get_graph_data, get_graph_stats, search_entities (already wired up)
- Existing components: EntityDetailPanel, SearchBar, StatsBar, Sidebar

ACCEPTANCE CRITERIA:
1. New component: src/components/ContentLibrary.jsx exists
2. App.jsx updated with view toggle (Library default, Graph optional)
3. Table renders all nodes from get_graph_data
4. Columns are sortable (click header to sort)
5. Search filters rows in real-time
6. Click row opens EntityDetailPanel
7. npm run build succeeds with zero errors

After each change, run: npm run build
If it fails, fix the error and try again.
"@
    }
    2 = @{
        Name = "Content Detail & Provenance Panel"
        Prompt = @"
You are building Nodalync Studio, a desktop app (Tauri 2.0 + React).

TASK: Build the Content Detail & Provenance Panel.

READ FIRST:
- $DESKTOP_DIR\STUDIO-PLAN.md
- $DESKTOP_DIR\src\components\EntityDetailPanel.jsx (existing, enhance it)
- $DESKTOP_DIR\rust-src\graph_commands.rs (get_subgraph, get_entity_content_links, get_context)

WHAT TO BUILD:
- Enhanced EntityDetailPanel with provenance visualization
- Show: all metadata, relationships list, source documents
- Provenance chain: visual tree showing L0→L1→L2→L3 derivation
- Version history section (using get_content_versions if available)
- Related entities section (from get_subgraph)
- Clean, functional layout — not fancy, just informative

ACCEPTANCE CRITERIA:
1. EntityDetailPanel enhanced with provenance section
2. Shows all entity metadata (type, label, description, source_count)
3. Lists all relationships (incoming and outgoing)
4. Provenance chain rendered as indented tree or breadcrumb
5. npm run build succeeds with zero errors
"@
    }
    3 = @{
        Name = "2D Entity Graph"
        Prompt = @"
You are building Nodalync Studio, a desktop app (Tauri 2.0 + React).

TASK: Replace the 3D graph with a clean 2D force-directed graph.

READ FIRST:
- $DESKTOP_DIR\STUDIO-PLAN.md
- $DESKTOP_DIR\src\components\graph3d\ (current 3D implementation — replacing this)
- $DESKTOP_DIR\src\lib\constants.js (entity colors, predicates)

WHAT TO BUILD:
- 2D force-directed graph using d3-force (NOT d3-force-3d, NOT React Three Fiber)
- Render on an HTML5 Canvas or SVG
- Nodes = entities, edges = relationships
- Consistent amber/gold color palette (NOT rainbow by entity type)
- Entity types distinguished by small icon or shape, not color
- Click node → triggers onNodeClick callback
- Hover → shows tooltip with label + type
- Zoom/pan with mouse wheel and drag
- Use forceX/forceY for containment (NOT forceCenter)

KEY: Keep it simple and functional. No 3D, no bloom, no glass shells.
This is a TOOL, not a screensaver.

ACCEPTANCE CRITERIA:
1. New component: src/components/Graph2D.jsx (or Graph2D/)
2. All 234 entities render as visible nodes
3. All 652 edges render as subtle lines
4. Click node triggers callback
5. Zoom and pan work
6. npm run build succeeds
"@
    }
}

if (-not $TASKS.ContainsKey($TaskNum)) {
    Write-Host "Unknown task number: $TaskNum. Available: $($TASKS.Keys -join ', ')"
    exit 1
}

$task = $TASKS[$TaskNum]
Write-Host "=== Ralph Wiggum Loop: Task $TaskNum - $($task.Name) ==="
Write-Host "Max iterations: $MaxIterations"

for ($i = 1; $i -le $MaxIterations; $i++) {
    Write-Host "`n--- Iteration $i/$MaxIterations ---"
    
    $iterPrompt = $task.Prompt
    if ($i -gt 1) {
        $iterPrompt += "`n`nThis is iteration $i. Check your previous work in the files. Fix any remaining issues. Run npm run build to verify."
    }
    
    # Run Claude Code with the task prompt
    claude --dangerously-skip-permissions -p $iterPrompt 2>&1
    
    # Check if build succeeds
    Write-Host "`n--- Build check ---"
    Set-Location $DESKTOP_DIR
    $buildResult = npm run build 2>&1
    $buildExit = $LASTEXITCODE
    
    if ($buildExit -eq 0) {
        Write-Host "BUILD PASSED on iteration $i"
        Write-Host "Task $TaskNum ($($task.Name)) complete!"
        exit 0
    } else {
        Write-Host "BUILD FAILED on iteration $i. Retrying..."
        Write-Host $buildResult | Select-Object -Last 10
    }
}

Write-Host "FAILED after $MaxIterations iterations"
exit 1
