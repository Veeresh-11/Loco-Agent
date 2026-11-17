"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.RoutingPolicy = void 0;
class RoutingPolicy {
    selectProvider(context) {
        const complexity = String(context.taskComplexity).toLowerCase();
        const highTokenCount = context.tokenCount > 12000;
        const complexTask = complexity === 'complex' || context.isCodeTask && context.tokenCount > 6000;
        if (context.preferLocal && context.tokenCount <= 6000 && !complexTask) {
            return 'local';
        }
        if (highTokenCount || complexTask) {
            return 'cloud';
        }
        if (context.isCodeTask && context.tokenCount <= 8000) {
            return 'local';
        }
        return 'local';
    }
    getRecommendedModel(context) {
        if (context.tokenCount > 12000 || context.taskComplexity === 'complex') {
            return 'gpt-4o-mini';
        }
        if (context.isCodeTask && context.tokenCount <= 6000) {
            return 'llama3.1';
        }
        return 'local-fast';
    }
}
exports.RoutingPolicy = RoutingPolicy;
//# sourceMappingURL=routingPolicy.js.map