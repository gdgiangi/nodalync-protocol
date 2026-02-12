# Nodalync Desktop App

A Tauri-based desktop application for the Nodalync Protocol, providing a graphical interface for node operation, network monitoring, and creator earnings tracking.

## Overview

This desktop app addresses the CLI-only limitation identified in the protocol FAQ (Section 17), providing a user-friendly GUI for:

- **Node Management**: Start, stop, configure, and monitor your Nodalync node
- **Network Overview**: Visualize network connections, DHT participation, and peer discovery
- **Creator Dashboard**: Track earnings, view payment channels, monitor content attribution
- **Settings & Configuration**: Manage node configuration, network settings, and preferences

## Architecture

### Technology Stack
- **Frontend**: HTML/CSS/JavaScript (or React/Vue.js)
- **Backend**: Tauri (Rust) - integrates with existing nodalync-cli
- **IPC**: Tauri commands invoke nodalync-cli processes
- **State Management**: Local app state with CLI data synchronization

### Integration Approach
The desktop app wraps and extends the existing CLI functionality rather than reimplementing protocol logic:

```
Desktop App (Tauri)
├── Frontend (HTML/JS/CSS)
│   ├── Dashboard View
│   ├── Network View 
│   ├── Settings View
│   └── System Tray
└── Rust Backend
    ├── CLI Command Wrappers
    ├── File System Watchers
    ├── Background Tasks
    └── Tauri Commands
```

### Command Integration
The app executes nodalync-cli commands and parses output:

```rust
// Example: Get node status
tauri::command
async fn get_node_status() -> Result<NodeStatus, String> {
    let output = Command::new("nodalync-cli")
        .args(["node", "status", "--json"])
        .output()
        .await?;
    
    let status: NodeStatus = serde_json::from_slice(&output.stdout)?;
    Ok(status)
}
```

## Development Setup

### Prerequisites
1. **Rust toolchain** (see main CONTRIBUTING.md)
2. **Node.js 18+** for frontend tooling
3. **Tauri CLI**: `cargo install tauri-cli`
4. **Working nodalync-cli** installation

### Building
```bash
cd apps/desktop
npm install           # Install frontend dependencies
cargo tauri dev       # Start development server
cargo tauri build     # Build production app
```

## User Interface Design

### Main Window
- **Header**: Node status indicator, network connectivity, sync status
- **Sidebar**: Navigation (Dashboard, Network, Settings, Help)
- **Main Content**: Context-dependent views

### Dashboard View
- Node uptime and status
- Recent earnings summary
- Active payment channels
- Network participation stats

### Network View
- Connected peers map/list
- DHT participation status
- Content routing visualization
- Network statistics

### Settings View
- Node configuration editor
- Network preferences
- Logging and debugging options
- Auto-update settings

### System Tray Integration
- Quick status overview
- Start/stop node
- Show main window
- Exit application

## Development Phases

### Phase 1: Foundation (Current)
- [ ] Tauri project setup and configuration
- [ ] Basic app structure with navigation
- [ ] CLI command integration architecture
- [ ] Simple dashboard with node status

### Phase 2: Core Features
- [ ] Network monitoring and visualization
- [ ] Creator earnings dashboard
- [ ] Configuration management UI
- [ ] System tray integration

### Phase 3: Enhancement
- [ ] Real-time updates and notifications
- [ ] Advanced network diagnostics
- [ ] Export/import functionality
- [ ] Help system and documentation

## Security Considerations

- **CLI Execution**: Sanitize all inputs to CLI commands
- **File Access**: Use Tauri's secure file system APIs
- **Network**: No direct protocol implementation - rely on CLI
- **Updates**: Use Tauri's secure update mechanism

## Contributing

This desktop app follows the same contribution guidelines as the main protocol. See the root CONTRIBUTING.md for development setup, testing, and code style guidelines.

## Status

**Current**: Foundation phase - project scaffolding and architecture design
**Target**: User-friendly desktop experience for the Nodalync Protocol

---

This desktop app provides the graphical interface that makes Nodalync accessible to creators and node operators who prefer GUIs over command-line tools.