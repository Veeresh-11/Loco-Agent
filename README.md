# LOCO Agent

A hybrid local + cloud AI assistant for Visual Studio Code that intelligently routes tasks between local LLMs and cloud providers for optimal performance and cost efficiency.

## Features

- 🚀 **Intelligent Routing**: Automatically chooses between local and cloud AI providers based on latency and cost
- 🔄 **Fallback Support**: Falls back to cloud provider if local response is unsatisfactory
- 🧠 **Multiple AI Providers**:
  - OpenAI GPT-4
  - Azure OpenAI
  - Local LLMs (Ollama, GPT4All)
- 📝 **Code Analysis**: Line-by-line code explanation
- 💡 **Smart Suggestions**: PowerShell and shell command suggestions
- 🔀 **Intelligent Patching**: Three-way merge with conflict detection and auto-apply patches
- ⚙️ **Configurable**: Flexible settings for provider selection and behavior
- 📊 **Job Queue**: Processes tasks concurrently with intelligent queuing

## Installation

1. Install the extension from VS Code Marketplace
2. Configure your AI provider in settings:

```json
{
  "loco.provider": "openai|azure|local|ollama|gpt4all",
  "loco.localEndpoint": "http://localhost:9000",
  "loco.autoApplyPatches": false
}
```

## Usage

### Commands

- **LOCO: Run Agent** - Execute an AI task on selected code
  - Explain code line-by-line
  - Suggest PowerShell to run
  - Auto-apply patch

- **LOCO: Open Assistant** - Open the LOCO webview for interactive assistance

- **LOCO: Open Settings** - Configure LOCO Agent preferences

### Quick Start

1. Open a file in VS Code
2. Select code or leave empty to use entire file
3. Run Command: `LOCO: Run Agent`
4. Choose task from the prompt
5. View results in the LOCO Output panel

## Architecture

### Core Components

- **LOCOAgent**: Main orchestrator managing job queue and provider routing
- **Job Queue**: Concurrent task processing with configurable concurrency
- **Routing Policy**: Intelligent selection between local/cloud based on latency and cost
- **Three-Way Merge**: Handles code patching with conflict detection

### Providers

- **CloudProvider**: Manages API calls to cloud AI services
- **LocalProvider**: Communicates with local LLM servers
- **ProviderManager**: Handles provider selection and configuration

### UI

- **Output View**: Real-time logging of agent operations
- **Settings Panel**: Configuration management
- **Webview Host**: Interactive assistant interface

## Project Structure

```text
src/
├── extension.ts              # Main extension entry point
├── core/
│   ├── locoAgent.ts         # Core agent orchestrator
│   ├── jobQueue.ts          # Concurrent job processing
│   ├── routingPolicy.ts     # Provider selection logic
│   └── threeWayMerge.apply.ts # Patch application
├── providers/
│   ├── cloudProvider.ts     # Cloud provider manager
│   ├── localProvider.ts     # Local LLM provider
│   ├── openaiProvider.ts    # OpenAI implementation
│   ├── azureProvider.ts     # Azure OpenAI implementation
│   ├── ollamaProvider.ts    # Ollama provider
│   ├── gpt4allProvider.ts   # GPT4All provider
│   └── providerManager.ts   # Provider selection
├── runners/
│   └── powershellRunner.ts  # PowerShell script execution
├── ui/
│   ├── outputView.ts        # Output logging
│   ├── settings.ts          # Settings panel
│   └── webview/             # Interactive UI
├── utils/
│   └── secrets.ts           # Secret management
└── __tests__/               # Test suites
```

## Configuration

### Settings

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `loco.provider` | string | `local` | Default AI provider (local, openai, azure, ollama, gpt4all) |
| `loco.localEndpoint` | string | `http://localhost:9000` | Local server endpoint for local LLMs |
| `loco.autoApplyPatches` | boolean | `false` | Automatically apply non-conflicting code patches |

## Development

### Prerequisites

- Node.js 16+
- npm or yarn
- TypeScript 5.0+

### Setup

```bash
npm install
npm run compile
npm test
```

### Scripts

- `npm run compile` - Compile TypeScript
- `npm run build:webview` - Build React webview
- `npm test` - Run test suite
- `npm run lint` - Run ESLint
- `npm run package` - Package as VS Code extension

## Testing

The project includes comprehensive test coverage:

- **Three-Way Merge Tests**: 8 test cases covering merge scenarios
  - Clean merge
  - Conflict detection
  - Local-only changes
  - Remote-only changes
  - Both sides adding/removing
  - Empty base handling
  - Same changes from both sides

- **Provider Tests**: 2 test cases
  - OpenAI provider JSON parsing
  - LocalProvider timeout handling

Run tests with:
```bash
npm test
```

## Build Status

- ✅ TypeScript: No compilation errors
- ✅ Tests: 10/10 passing
- ✅ Ready for production deployment

## License

MIT

## Contributing

Contributions are welcome! Please ensure all tests pass and code is properly typed.

## Support

For issues, feature requests, or questions, please open an issue in the repository.

---

## History & Privacy

- **Local history:** The extension stores recent LOCO interactions in VS Code's `globalState` (by default, retained for 30 days). This history is used to provide conversational context to local LLMs.
- **Settings:** You can control history retention and behavior in Settings (see below). To view, export, or clear stored history use the commands: `LOCO: View History`, `LOCO: Export History`, `LOCO: Clear History`.
- **Privacy:** If you prefer not to store history, disable `locoAgent.storeHistory` in settings. History is stored locally in the editor state and is not transmitted anywhere by default other than the configured model endpoints.

## Settings (important)

The extension exposes several `locoAgent.*` settings. Key options:

- `locoAgent.provider` (string, default `local`): Default provider (local, openai, azure, ollama, gpt4all).
- `locoAgent.endpoint` (string): Explicit local endpoint URL if you prefer not to use auto-detection.
- `locoAgent.localPorts` (number[]): Ports to probe when auto-detecting a local provider (defaults: 9000, 9001, 8080, 7860, 5005).
- `locoAgent.storeHistory` (boolean, default `true`): Toggle local storage of recent LOCO interactions.
- `locoAgent.pruneOnStartup` (boolean, default `true`): Whether to prune history older than `locoAgent.pruneDays` when the extension activates.
- `locoAgent.pruneDays` (number, default `30`): Retention window (days) for history pruning.
- `locoAgent.maxHistoryEntries` (number, default `1000`): Maximum number of entries kept after pruning.

## Commands (quick reference)

- `LOCO: Run Agent` — Run a task on selected code or whole file.
- `LOCO: Open Assistant` — Open the webview chat assistant.
- `LOCO: Open Settings` — Open LOCO settings and choose provider.
- `LOCO: Set Local Endpoint` — Manually set a local LLM endpoint.
- `LOCO: Clear Local Endpoint` — Clear stored endpoint (auto-detect next time).
- `LOCO: View History` — View recent stored LOCO interactions.
- `LOCO: Export History` — Export stored history to a JSON file.
- `LOCO: Clear History` — Clear stored LOCO history.

## Packaging & Publishing

To build and package the extension locally:

```powershell
npm install
npm run compile
npm run build:webview
npm run package
```

This produces a `.vsix` package in the workspace root. To publish to the Visual Studio Marketplace:

1. Create a publisher at https://marketplace.visualstudio.com/manage.
2. Install `vsce` and follow the authentication steps:

```powershell
npm i -g vsce
vsce login <publisher-name>
```

3. Publish (after logging in):

```powershell
vsce publish
```

(You can also use `npx vsce package` to create a local package without publishing.)

---

**LOCO Agent** - Making AI smarter, faster, and cheaper for developers.
