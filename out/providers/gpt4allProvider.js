"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.GPT4AllProvider = void 0;
const localProvider_1 = require("./localProvider");
class GPT4AllProvider {
    local;
    constructor(context) {
        this.local = new localProvider_1.LocalProvider(context);
    }
    async ask(prompt) {
        const result = await this.local.ask({ prompt, provider: 'gpt4all', model: 'gpt4all' });
        const text = typeof result === 'string' ? result : result?.text;
        if (!text)
            throw new Error('GPT4All was not detected or returned no response.');
        return text;
    }
}
exports.GPT4AllProvider = GPT4AllProvider;
//# sourceMappingURL=gpt4allProvider.js.map