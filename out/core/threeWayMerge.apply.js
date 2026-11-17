"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.applyThreeWayMerge = applyThreeWayMerge;
function applyThreeWayMerge(job) {
    const baseLines = (job.base ?? '').split(/\r?\n/);
    const localLines = (job.local ?? '').split(/\r?\n/);
    const remoteLines = (job.remote ?? '').split(/\r?\n/);
    const merged = [];
    let conflicts = 0;
    const maxLen = Math.max(baseLines.length, localLines.length, remoteLines.length);
    for (let i = 0; i < maxLen; i += 1) {
        const baseLine = baseLines[i] ?? '';
        const localLine = localLines[i] ?? '';
        const remoteLine = remoteLines[i] ?? '';
        if (localLine === remoteLine) {
            merged.push(localLine);
        }
        else if (baseLine === localLine) {
            merged.push(remoteLine);
        }
        else if (baseLine === remoteLine) {
            merged.push(localLine);
        }
        else {
            merged.push(`<<<<<<< LOCAL\n${localLine}\n=======\n${remoteLine}\n>>>>>>> REMOTE`);
            conflicts += 1;
        }
    }
    return {
        merged: merged.join('\n'),
        conflicts
    };
}
//# sourceMappingURL=threeWayMerge.apply.js.map