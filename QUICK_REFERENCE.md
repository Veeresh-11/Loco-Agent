# LOCO Agent - Quick Reference

## Installation

**From VS Code Marketplace**:
1. Open Extensions: `Ctrl+Shift+X`
2. Search "LOCO Agent"
3. Click Install

## Quick Commands

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+P` | Open Command Palette |
| Type: `LOCO: Run Agent` | Execute AI task |
| `Ctrl+Shift+X` | Open Extensions |
| `Ctrl+,` | Open Settings |

## Configure Provider

1. `Ctrl+Shift+P` → "Preferences: Open Settings (JSON)"
2. Add:
```json
{
  "loco.provider": "openai",
  "loco.localEndpoint": "http://localhost:9000",
  "loco.autoApplyPatches": false
}
```

## Available Providers

| Provider | Speed | Quality | Cost | Setup |
|----------|-------|---------|------|-------|
| **Local** | ⚡⚡⚡ | ⭐⭐⭐ | Free | Ollama/GPT4All |
| **OpenAI** | ⚡⚡ | ⭐⭐⭐⭐⭐ | $0.01/task | API Key |
| **Azure** | ⚡⚡ | ⭐⭐⭐⭐⭐ | Varies | Azure account |
| **Ollama** | ⚡⚡⚡ | ⭐⭐⭐⭐ | Free | Download |
| **GPT4All** | ⚡⚡⚡ | ⭐⭐⭐ | Free | Download |

## Usage Workflow

```
1. Select Code
   ↓
2. Press Ctrl+Shift+P
   ↓
3. Type "LOCO: Run Agent"
   ↓
4. Choose Task:
   • Explain code line-by-line
   • Suggest PowerShell to run
   • Auto-apply patch
   ↓
5. View Results in LOCO Output
   ↓
6. Review & Apply Changes
```

## Task Types

### 🔍 Explain Code
- View detailed explanation
- Understand logic flow
- Learn best practices
- Generate documentation

### ⚙️ Suggest PowerShell
- Auto-generate scripts
- DevOps automation
- System administration
- Batch operations

### 🔀 Auto-Apply Patch
- Smart code refactoring
- Bug fixes
- Improvements
- Three-way merge with conflict detection

## Output Panel

Access results:
1. `Ctrl+`` ` (backtick) → Toggle Terminal
2. Look for "LOCO Agent" tab
3. See real-time results
4. Copy results to clipboard

## Performance Tips

| Scenario | Best Provider | Settings |
|----------|---------------|----------|
| **Sensitive code** | Local | Use Ollama |
| **Best quality** | OpenAI | gpt-4 model |
| **Fast response** | Local | GPT4All |
| **Enterprise** | Azure | Compliance ready |
| **Cost-conscious** | Local | Zero cost |

## Troubleshooting

### Command not found
→ Reload VS Code: `Ctrl+Shift+P` → Reload Window

### Local server won't connect
→ Start Ollama: `ollama serve`

### API key error
→ Set env var: `setx OPENAI_API_KEY "sk-..."`

### Slow response
→ Use local provider for speed

## Documentation Files

- 📖 **README.md** - Full feature documentation
- 📘 **USER_MANUAL.md** - Complete user guide
- 💡 **USE_CASES.md** - Real-world examples
- 🚀 **DEPLOYMENT.md** - Publishing guide
- 📋 **QUICK_REFERENCE.md** - This file

## Resources

- GitHub: https://github.com/yourusername/loco-agent
- Issues: https://github.com/yourusername/loco-agent/issues
- Discussions: https://github.com/yourusername/loco-agent/discussions

## Keyboard Shortcuts Reference

```
Ctrl+Shift+P     Command Palette
Ctrl+Shift+X     Extensions Panel
Ctrl+,           Settings
Ctrl+K Ctrl+S    Keyboard Shortcuts
Ctrl+``          Terminal/Output
Ctrl+R           Reload Window
Ctrl+Shift+L     Format Selection
Ctrl+Z           Undo
Ctrl+Shift+Z     Redo
```

## First Time Setup

1. Install extension
2. Open a code file
3. Run "LOCO: Run Agent"
4. Choose your provider in Settings
5. Try first task
6. Check Output panel for results

## Need Help?

1. Read USER_MANUAL.md
2. Check USE_CASES.md for examples
3. Open issue on GitHub
4. Post in discussions

---

**Start using LOCO Agent now!** Happy coding! 🚀
