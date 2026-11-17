"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.Secrets = void 0;
class Secrets {
    static key(service) {
        return `loco.${service}.apiKey`;
    }
    static async getApiKey(context, service) {
        try {
            return await context.secrets.get(this.key(service));
        }
        catch {
            return undefined;
        }
    }
    static async setApiKey(context, service, key) {
        try {
            if (!key.trim()) {
                await this.clearApiKey(context, service);
                return;
            }
            await context.secrets.store(this.key(service), key.trim());
        }
        catch (error) {
            throw new Error(`Could not securely store the ${service} API key: ${String(error)}`);
        }
    }
    static async clearApiKey(context, service) {
        await context.secrets.delete(this.key(service));
    }
}
exports.Secrets = Secrets;
//# sourceMappingURL=secrets.js.map