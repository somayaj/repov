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
| `g` / `G` | Jump to top / bottom of list |
| `PgUp` / `PgDn` | Page up / down |
| `/` or `f` | Search commits (message, author, sha) |
| `Enter` | Open commit or file diff |
| `b` | Branch line view (linear history) |
| `p` | Toggle branch line / full merge graph |
| `h` | Show / hide commit graph column |
| `c` | Cycle files: changed → all → working tree |
| `y` | Copy commit SHA to clipboard |
| `r` | Reload repository |
| `?` | Toggle help overlay |
| `Esc` | Close diff / search / help |
| `q` | Quit |

## License

MIT
