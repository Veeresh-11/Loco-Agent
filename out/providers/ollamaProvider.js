"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.OllamaProvider = void 0;
const localProvider_1 = require("./localProvider");
class OllamaProvider {
    local;
    constructor(context) {
        this.local = new localProvider_1.LocalProvider(context);
    }
    async ask(prompt) {
        const result = await this.local.ask({ prompt, provider: 'ollama' });
        const text = typeof result === 'string' ? result : result?.text;
        if (!text)
            throw new Error('Ollama was not detected or returned no response.');
        return text;
    }
}
exports.OllamaProvider = OllamaProvider;
//# sourceMappingURL=ollamaProvider.js.map