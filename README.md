# FTTUI - FlavorTown Project Viewer TUI

**FTTUI** is a terminal-based user interface (TUI) for browsing projects from the FlavorTown Public API by jam06452. It allows users to explore hot, top (this week), top (all time) and random projects right into your shell!
## Features

- Browse **Hot projects**, **Top this week**, **Top all time**, and **Random projects** in a 4-panel TUI layout.  
- View project **details**, including description, repo URL, demo URL, and ship status.  
- Automatic **refresh** from the API with configurable interval.  
- Cross-platform: works on **Linux** and **Windows** terminals.  

## Use Case

FTTUI is designed for the optimisation sidequest and makes it fun and easy to explore projects you would otherwise had never seen. 

It’s perfect for:  

- Quickly checking trending projects.  
- Find new projects to take a look at 

## Installation

### Linux / Windows Binaries

1. Download the appropriate binary or `.exe` for your system from the releases.  
2. Run it directly from the terminal (or you double click it if you are on windows):  

```bash
./fttui      # Linux
fttui.exe    # Windows
```

## Build from Source

Make sure you have Rust installed. Then:

```bash
git clone https://github.com/QKing-Official/FTTUI
cd FTTUI
cargo build --release
```
The compiled binary will be in the target folder.

## Configuration

A configuration file is automatically created at:

~/.config/fttui/config.json

Default configuration:

```json
{
  "refresh_seconds": 30
}
```
## Optimization

### Memory Usage

We checked memory while FTTUI ran in the background using `ps`:
```bash
PID     RSS     VSZ    COMMAND  
73689   4256    2105004 fttui   # first run, cold cache  
73906   4300    2105004 fttui   # second run, cached  
```

- First run (cold cache) → lots of HTTPS requests to fetch stuff.  
- Second run (warm cache) → just a few requests, showing cache is working.  
- Packets captured: 11 packets on cached run vs many more on first run.

### Performance Timing

We measured how long it takes with `time`:

# First run (cold cache)  
real    0m0.793s  

# Second run (warm cache)  
real    0m0.319s  

Cached runs are much faster. Release build makes it even quicker.

### Tools & Metrics Used

| Metric            | Tool / Command |
|------------------|----------------|
| Memory (RSS, VSZ) | `ps -p <pid> -o pid,rss,vsz,comm` |
| Network requests  | `tcpdump -i any port 80 or port 443` |
| Execution time    | `time target/release/fttui` |

## License

This project is licensed under MIT
Feel free to contribute!