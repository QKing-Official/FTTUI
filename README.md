# FTTUI - FlavorTown Project Viewer TUI

**FTTUI** is a terminal-based user interface (TUI) for browsing projects from the FlavorTown API. It allows users to explore “hot,” weekly top, all-time top, and random projects, view details, and scroll through information directly in the terminal.  

## Features

- Browse **Hot projects**, **Top this week**, **Top all time**, and **Random projects** in a 4-panel TUI layout.  
- View project **details**, including description, repo URL, demo URL, and ship status.  
- Automatic **refresh** from the API with configurable interval.  
- Cross-platform: works on **Linux** and **Windows** terminals.  

## Use Case

FTTUI is designed for developers, hobbyists, and enthusiasts who want a quick terminal interface to explore projects from the FlavorTown community without leaving the terminal.  

It’s perfect for:  

- Quickly checking trending projects.  
- Browsing project details on-the-go.  
- Lightweight, distraction-free project exploration.  

## Installation

### Linux / Windows Binaries

1. Download the appropriate binary or `.exe` for your system.  
2. Run it directly from the terminal:  

```bash
./fttui      # Linux
fttui.exe    # Windows
```

## Build from Source

Make sure you have Rust installed. Then:

```bash
git clone <repo_url>
cd fttui
cargo build --release
```
The compiled binary will be in target/release/fttui.

## Controls

- Arrow keys / W/S – Navigate through the project list.
- Tab – Switch between panels.
- Enter / Space – Open project details.
- Esc / Backspace – Close project details.
- r – Refresh all panels from API.
- q – Quit the application.

## Configuration

A configuration file is automatically created at:

~/.config/fttui/config.json

Default configuration:

```json
{
  "refresh_seconds": 30
}

