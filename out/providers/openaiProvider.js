"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.OpenAIProvider = void 0;
const cloudProvider_1 = require("./cloudProvider");
class OpenAIProvider extends cloudProvider_1.CloudProvider {
    constructor(context) {
        super(context);
    }
    async ask(prompt) {
        return super.ask(prompt, { model: 'gpt-4o-mini', temperature: 0.2, maxTokens: 1200 });
    }
}
exports.OpenAIProvider = OpenAIProvider;
//# sourceMappingURL=openaiProvider.js.map