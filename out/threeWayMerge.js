"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.threeWayMerge = threeWayMerge;
function threeWayMerge(base, local, remote) {
    const baseLines = base.split('\n');
    const localLines = local.split('\n');
    const remoteLines = remote.split('\n');
    const merged = [];
    const conflicts = [];
    // Simple three-way merge: if both changed the same, it's a conflict
    const maxLen = Math.max(baseLines.length, localLines.length, remoteLines.length);
    for (let i = 0; i < maxLen; i++) {
        const baseLine = baseLines[i] || '';
        const localLine = localLines[i] || '';
        const remoteLine = remoteLines[i] || '';
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
            // Conflict
            merged.push(`<<<<<<< LOCAL\n${localLine}\n=======\n${remoteLine}\n>>>>>>>`);
            conflicts.push({
                marker: `line ${i + 1}`,
                base: baseLine,
                local: localLine,
                remote: remoteLine,
            });
        }
    }
    return {
        merged: merged.join('\n'),
        conflicts,
    };
}
//# sourceMappingURL=threeWayMerge.js.map