# LOCO Agent - User Manual

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Commands & Features](#commands--features)
4. [Configuration](#configuration)
5. [Providers Setup](#providers-setup)
6. [Tips & Tricks](#tips--tricks)
7. [FAQ](#faq)

## Installation

### From VS Code Marketplace

1. Open VS Code
2. Press `Ctrl+Shift+X` to open Extensions panel
3. Search for "LOCO Agent"
4. Click **Install**
5. Reload VS Code when prompted

### From VSIX File

```bash
code --install-extension loco-agent-0.1.0.vsix
```

## Quick Start

### 1. First Run

After installation, LOCO Agent is ready to use. Open any code file and:

1. **Select code** (or leave empty to use entire file)
2. Open Command Palette: `Ctrl+Shift+P`
3. Type "LOCO: Run Agent"
4. Choose a task from the menu

### 2. Your First Task

**Example: Explain Code Line-by-Line**

```python
# Select this code
def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)
```

1. Select the code
2. Run "LOCO: Run Agent"
3. Choose "Explain code line-by-line"
4. View results in LOCO Output panel

## Commands & Features

### Available Commands

Press `Ctrl+Shift+P` and search for "LOCO":

| Command | Description | Icon |
|---------|-------------|------|
| **LOCO: Run Agent** | Execute AI task on selected code | ⚙️ |
| **LOCO: Open Assistant** | Open interactive webview | 💬 |
| **LOCO: Open Settings** | Configure LOCO preferences | ⚙️ |

### Task Types

#### 1. Explain Code Line-by-Line

- **Use Case**: Understanding unfamiliar code
- **Input**: Code selection or entire file
- **Output**: Detailed explanation in LOCO Output panel
- **Best For**: Learning, documentation, code reviews

**Example**:
```javascript
// Input: Array.prototype.map implementation
const result = [1, 2, 3].map(x => x * 2);

// Output: Line-by-line explanation with variable tracking
```

#### 2. Suggest PowerShell to Run

- **Use Case**: Generate shell commands from description
- **Input**: Code or natural language request
- **Output**: PowerShell script suggestion
- **Best For**: Windows automation, DevOps tasks

**Example**:
```
// Tell LOCO: "Create a backup of all .js files"
// Get: PowerShell script to compress and backup files
```

#### 3. Auto-Apply Patch

- **Use Case**: Apply AI-suggested code changes
- **Input**: Code with suggestions
- **Output**: Three-way merge with conflict detection
- **Best For**: Code refactoring, bug fixes, improvements

**Example**:
```javascript
// Before: function oldCode() { ... }
// LOCO suggests: Modernize to arrow function
// Apply: Auto-merges with conflict warnings if needed
```

## Configuration

### Access Settings

1. Open VS Code Settings: `Ctrl+,`
2. Search for "LOCO"
3. Configure options:

### Available Settings

#### 1. Provider Selection

| Setting | Values | Default | Description |
|---------|--------|---------|-------------|
| `loco.provider` | local, openai, azure, ollama, gpt4all | local | Which AI provider to use |

**Options**:
- `local` - Run on your machine (fast, private)
- `openai` - Use OpenAI GPT-4 (best quality)
- `azure` - Use Azure OpenAI (enterprise)
- `ollama` - Use Ollama local models
- `gpt4all` - Use GPT4All (lightweight)

#### 2. Local Server Endpoint

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `loco.localEndpoint` | string | http://localhost:9000 | Local LLM server URL |

**When to change**:
- Local server on different port: `http://localhost:8080`
- Remote server: `http://192.168.1.100:9000`

#### 3. Auto-Apply Patches

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `loco.autoApplyPatches` | boolean | false | Auto-apply non-conflicting patches |

**Options**:
- `false` - Review patches before applying (recommended)
- `true` - Auto-apply clean patches

## Providers Setup

### Cloud Providers

#### OpenAI

1. Get API key: https://platform.openai.com/api-keys
2. Set environment variable:
   ```bash
   # Windows
   setx OPENAI_API_KEY "sk-..."
   
   # macOS/Linux
   export OPENAI_API_KEY="sk-..."
   ```
3. Set provider: `loco.provider = "openai"`

**Cost**: ~$0.01 per task

#### Azure OpenAI

1. Create Azure account
2. Deploy Azure OpenAI resource
3. Set in VS Code settings:
   ```json
   {
     "loco.provider": "azure"
   }
   ```

### Local Providers

#### Ollama (Recommended for Local)

1. Download Ollama: https://ollama.ai
2. Install and run:
   ```bash
   ollama serve
   ```
3. Pull a model:
   ```bash
   ollama pull mistral
   ollama pull neural-chat
   ```
4. Set provider: `loco.provider = "ollama"`

**Advantage**: Free, no API keys, completely private

#### GPT4All

1. Download GPT4All: https://gpt4all.io
2. Install and configure
3. Start server on port 9000
4. Set provider: `loco.provider = "gpt4all"`

## Tips & Tricks

### 1. Keyboard Shortcuts

```
Ctrl+Shift+P     → Command Palette (Run LOCO Agent)
Ctrl+Shift+X     → Extensions panel
Ctrl+,           → Settings
Ctrl+`           → Toggle terminal (see LOCO Output)
```

### 2. Smart Selection

- **Select code block**: Triple-click a line
- **Select function**: Click function name, Ctrl+Shift+L
- **Select all**: Ctrl+A (uses entire file)

### 3. Task Chaining

1. Run "Explain code"
2. Read output
3. Run "Suggest improvement"
4. Apply patches
5. Repeat for next section

### 4. Provider Switching

Switch providers mid-session to compare results:

1. Change `loco.provider` setting
2. Run same task again
3. Compare outputs
4. Choose best result

### 5. Working with Large Files

For large files:
- Select specific functions/blocks
- Don't analyze entire file at once
- Use local provider for speed

### 6. Output Panel

Click "LOCO Agent" in Output panel to:
- See real-time task progress
- Copy results
- Clear history
- Export logs

## FAQ

### Q: Is my code private?

**A**: Depends on provider:
- **Local (Ollama/GPT4All)**: 100% private, runs on your machine
- **Cloud (OpenAI/Azure)**: Check provider's privacy policy
- **Recommendation**: Use local for sensitive code

### Q: How much does it cost?

**A**: 
- **Local**: Free (one-time installation)
- **OpenAI**: ~$0.01 per task
- **Azure**: Based on your pricing plan
- **Ollama/GPT4All**: Free

### Q: Which provider is fastest?

**A**: In order:
1. **GPT4All** (local, fastest)
2. **Ollama** (local, good quality)
3. **OpenAI** (cloud, high quality)
4. **Azure** (cloud, enterprise)

### Q: Can I use multiple providers?

**A**: Yes! Switch `loco.provider` setting to try different providers.

### Q: What if LOCO runs out of token limits?

**A**: Use local provider - no limits!

### Q: How do I report bugs?

**A**: Open issue at: https://github.com/yourusername/loco-agent/issues

### Q: Can I contribute?

**A**: Yes! Fork repo and submit PR.

### Q: Does it work offline?

**A**: Yes, if using local providers (Ollama, GPT4All).

### Q: What's the minimum file size?

**A**: Works with any size - optimal for 10-500 lines per task.

## Troubleshooting

### Problem: Extension doesn't appear

**Solution**:
1. Check VS Code version: 1.80.0+
2. Reload VS Code: `Ctrl+R`
3. Check Extensions panel for errors
4. Restart VS Code

### Problem: Commands not working

**Solution**:
1. Open Command Palette: `Ctrl+Shift+P`
2. Type "Developer: Reload Window"
3. Check LOCO Output for errors
4. Verify provider is configured

### Problem: Local server won't connect

**Solution**:
1. Start local server: `ollama serve`
2. Verify endpoint: Default is http://localhost:9000
3. Check firewall settings
4. Verify server is actually running

### Problem: API key errors

**Solution**:
1. Verify API key is correct
2. Check environment variable is set: `echo $OPENAI_API_KEY`
3. Restart VS Code after setting key
4. Check API key hasn't expired

### Problem: Slow responses

**Solution**:
1. Switch to local provider for speed
2. Use smaller code selections
3. Check network connection
4. Verify API service status

## Next Steps

- 📖 Read [USE_CASES.md](USE_CASES.md) for real-world examples
- 🚀 Read [DEPLOYMENT.md](DEPLOYMENT.md) to publish your own version
- 💬 Join community discussions
- ⭐ Star the GitHub repository

---

**Happy coding with LOCO Agent!** 🚀
