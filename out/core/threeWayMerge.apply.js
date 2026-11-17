"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.applyThreeWayMerge = applyThreeWayMerge;
function applyThreeWayMerge(job) {
    if (job.local === job.remote)
        return { merged: job.local ?? '', conflicts: 0 };
    if (job.local === job.base)
        return { merged: job.remote ?? '', conflicts: 0 };
    if (job.remote === job.base)
        return { merged: job.local ?? '', conflicts: 0 };
    const base = splitLines(job.base);
    const changes = [
        ...findChanges(base, splitLines(job.local), 'local'),
        ...findChanges(base, splitLines(job.remote), 'remote')
    ].sort((a, b) => a.start - b.start || a.end - b.end || (a.side === 'local' ? -1 : 1));
    const output = [];
    let cursor = 0;
    let conflicts = 0;
    let index = 0;
    while (index < changes.length) {
        const group = [changes[index++]];
        let start = group[0].start;
        let end = group[0].end;
        let expanded = true;
        while (expanded) {
            expanded = false;
            while (index < changes.length && overlaps(start, end, changes[index])) {
                const change = changes[index++];
                group.push(change);
                start = Math.min(start, change.start);
                end = Math.max(end, change.end);
                expanded = true;
            }
        }
        output.push(...base.slice(cursor, start));
        const localChanges = group.filter(change => change.side === 'local');
        const remoteChanges = group.filter(change => change.side === 'remote');
        const localResult = applyChanges(base, start, end, localChanges);
        const remoteResult = applyChanges(base, start, end, remoteChanges);
        if (!localChanges.length)
            output.push(...remoteResult);
        else if (!remoteChanges.length || arraysEqual(localResult, remoteResult))
            output.push(...localResult);
        else {
            output.push('<<<<<<< LOCAL', ...localResult, '=======', ...remoteResult, '>>>>>>> REMOTE');
            conflicts += 1;
        }
        cursor = end;
    }
    output.push(...base.slice(cursor));
    return { merged: output.join('\n'), conflicts };
}
function splitLines(value) {
    return (value ?? '').split(/\r?\n/);
}
function findChanges(base, variant, side) {
    const n = base.length;
    const m = variant.length;
    // Avoid allocating an unbounded LCS matrix for very large files.
    if (n * m > 2_000_000) {
        let prefix = 0;
        while (prefix < n && prefix < m && base[prefix] === variant[prefix])
            prefix += 1;
        let suffix = 0;
        while (suffix < n - prefix && suffix < m - prefix && base[n - 1 - suffix] === variant[m - 1 - suffix])
            suffix += 1;
        return [{ start: prefix, end: n - suffix, replacement: variant.slice(prefix, m - suffix), side }];
    }
    const width = m + 1;
    const lcs = new Uint32Array((n + 1) * width);
    for (let i = n - 1; i >= 0; i -= 1) {
        for (let j = m - 1; j >= 0; j -= 1) {
            lcs[i * width + j] = base[i] === variant[j]
                ? lcs[(i + 1) * width + j + 1] + 1
                : Math.max(lcs[(i + 1) * width + j], lcs[i * width + j + 1]);
        }
    }
    const changes = [];
    let i = 0;
    let j = 0;
    let start = -1;
    let replacement = [];
    const flush = () => {
        if (start >= 0)
            changes.push({ start, end: i, replacement, side });
        start = -1;
        replacement = [];
    };
    while (i < n || j < m) {
        if (i < n && j < m && base[i] === variant[j]) {
            flush();
            i += 1;
            j += 1;
        }
        else {
            if (start < 0)
                start = i;
            if (j < m && (i === n || lcs[i * width + j + 1] >= lcs[(i + 1) * width + j])) {
                replacement.push(variant[j++]);
            }
            else {
                i += 1;
            }
        }
    }
    flush();
    return changes;
}
function overlaps(start, end, change) {
    if (start === end && change.start === change.end)
        return start === change.start;
    if (change.start === change.end)
        return change.start > start && change.start < end;
    if (start === end)
        return start > change.start && start < change.end;
    return change.start < end && change.end > start;
}
function applyChanges(base, start, end, changes) {
    const result = [];
    let cursor = start;
    for (const change of changes) {
        result.push(...base.slice(cursor, change.start), ...change.replacement);
        cursor = change.end;
    }
    result.push(...base.slice(cursor, end));
    return result;
}
function arraysEqual(left, right) {
    return left.length === right.length && left.every((line, index) => line === right[index]);
}
//# sourceMappingURL=threeWayMerge.apply.js.map