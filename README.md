# repov

A gitk-style terminal UI for exploring git repositories.

## Install

```bash
cargo install repov
```

## Usage

```bash
repov              # current directory
repov /path/to/repo
```

## Keys

| Key | Action |
|-----|--------|
| `Tab` / `Shift+Tab` | Switch panel (Refs → History → Files) |
| `j` / `k` | Move up/down |
| `Enter` | Open commit or file diff |
| `c` | Toggle changed files / all files |
| `y` | Copy commit SHA to clipboard |
| `r` | Reload repository |
| `Esc` | Close diff view |
| `q` | Quit |

## License

MIT
