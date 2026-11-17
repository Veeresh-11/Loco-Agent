# LOCO Agent - Deployment & Publishing Guide

## Overview

LOCO Agent is a production-ready VS Code extension providing Copilot-like inline code completions with intelligent local/cloud provider routing, 30-day history persistence, and smart fallback support.

## Pre-Deployment Checklist

✅ **Completed Tasks**
- TypeScript compilation: No errors
- All 10 unit tests passing
- Inline completion provider with streaming & cancellation
- Local provider with automatic port failover (TCP probe + HTTP health check)
- 30-day conversation history persistence with optional storage
- Accept/next suggestion keybindings
- Chat panel webview (minimized UI)
- Settings UI for provider selection and configuration
- History view, export, and clear commands
- Auto-prune history on activation with configurable retention
- README & CHANGELOG updated

## Publishing to VS Code Marketplace

### Prerequisites

1. **Visual Studio Code Account**
   - Create account at https://dev.azure.com
   - Create a personal access token (PAT) with "Marketplace (manage)" scope

2. **Publisher Account**
   - Register as a publisher at https://marketplace.visualstudio.com
   - Publisher ID: **veeresh**

3. **Required Tools**
   ```powershell
   npm install -g vsce
   ```

### Step 1: Install vsce

```powershell
npm install -g vsce
vsce --version  # Verify installation
```

### Step 2: Create Publisher Account (One-time)

```powershell
# Create publisher
vsce create-publisher veeresh -p YOUR_PAT_TOKEN

# Or login to existing publisher
vsce login veeresh
```

### Step 3: Create .vscodeignore

Create `.vscodeignore` file in project root:

```
.git
.gitignore
.vscode
.vscode-test
jest.config.js
tsconfig.json
tsconfig.tsbuildinfo
src
node_modules
dist
*.vsix
.prettierrc
.eslintrc
DEPLOYMENT.md
USER_MANUAL.md
USE_CASES.md
```

### Step 4: Build Extension

```bash
# Clean build
npm run compile

# Build webview (if using React UI)
npm run build:webview

# Ensure tests pass
npm test
```

### Step 5: Package Extension

```bash
# Package as VSIX file
vsce package

# This creates: loco-agent-0.1.0.vsix
```

### Step 6: Publish to Marketplace

```bash
# Publish to VS Code Marketplace
vsce publish

# Or publish specific version
vsce publish 0.1.0
```

### Alternative: Manual Upload

1. Go to https://marketplace.visualstudio.com/manage/publishers/veeresh
2. Click "New extension"
3. Upload the `.vsix` file
4. Fill in marketplace details
5. Click "Create"

## Marketplace Listing Optimization

### Icon & Images

- **Icon**: 128x128 PNG (already added: `media/loco-agent-colored-128x128.png`)
- **Preview**: Create a high-quality screenshot showing the extension in action
- **Repository**: Link to GitHub repository

### Marketplace Description

Add to `package.json`:

```json
{
  "keywords": [
    "ai",
    "copilot",
    "code-completion",
    "local-ai",
    "cloud-ai",
    "llm",
    "openai",
    "azure",
    "ollama",
    "gpt4all"
  ],
  "categories": [
    "AI",
    "Machine Learning",
    "Programming Languages"
  ]
}
```

## Installation Methods for End Users

### Method 1: VS Code Marketplace (Easiest)

1. Open VS Code
2. Go to Extensions (Ctrl+Shift+X)
3. Search for "LOCO Agent"
4. Click "Install"

### Method 2: Install from VSIX

```bash
# Download loco-agent-0.1.0.vsix
code --install-extension loco-agent-0.1.0.vsix
```

### Method 3: Install from Command Line

```bash
# For published extension
code --install-extension veeresh.loco-agent
```

## Post-Installation Setup

Once installed, users need to:

1. **Configure Provider**
   - Open Settings (Ctrl+,)
   - Search for "LOCO"
   - Set preferred provider

2. **Configure API Keys** (for cloud providers)
   - OpenAI: Set `OPENAI_API_KEY` environment variable
   - Azure: Set Azure credentials in VS Code settings

3. **Local Server Setup** (if using local provider)
   - Install Ollama or GPT4All
   - Start local server on port 9000

## Troubleshooting

### Extension Not Appearing

1. Check VS Code version is 1.80.0+
2. Reload VS Code (Ctrl+R)
3. Check Extension tab for errors

### Commands Not Working

1. Open Command Palette (Ctrl+Shift+P)
2. Type "LOCO: Run Agent"
3. Check Output panel for errors

### Compilation Issues

```bash
# Clear cache
rm -rf out/
npm run compile
```

## Update & Versioning

```bash
# Update version in package.json
# 0.1.0 → 0.1.1 (patch)
# 0.1.0 → 0.2.0 (minor)
# 0.1.0 → 1.0.0 (major)

# Rebuild and publish
npm run compile
npm test
vsce publish
```

## Monitoring & Analytics

After publishing, monitor:
- Marketplace views and installs
- User ratings and reviews
- GitHub issues and PRs
- Performance metrics

## Support & Community

- **Documentation**: README.md in repository
- **Issues**: GitHub issues page
- **Discussions**: GitHub discussions
- **Email Support**: support@your-domain.com
