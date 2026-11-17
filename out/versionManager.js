"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VersionManager = void 0;
class VersionManager {
    context;
    currentVersion;
    constructor(context) {
        this.context = context;
        this.currentVersion = context.extension.packageJSON.version || '0.1.0';
    }
    getVersion() {
        return this.currentVersion;
    }
    async checkForUpdates() {
        const lastSeenVersion = this.context.globalState.get('lastSeenVersion');
        if (lastSeenVersion !== this.currentVersion) {
            await this.context.globalState.update('lastSeenVersion', this.currentVersion);
            return true;
        }
        return false;
    }
}
exports.VersionManager = VersionManager;
//# sourceMappingURL=versionManager.js.map