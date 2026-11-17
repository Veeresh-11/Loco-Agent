"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const threeWayMerge_apply_1 = require("../core/threeWayMerge.apply");
describe('Three-Way Merge', () => {
    test('clean merge', () => {
        const result = (0, threeWayMerge_apply_1.applyThreeWayMerge)({
            base: 'a\nb\nc',
            local: 'a\nx\nc',
            remote: 'a\nb\ny',
        });
        expect(result.conflicts).toBe(0);
    });
    test('detects conflicts', () => {
        const result = (0, threeWayMerge_apply_1.applyThreeWayMerge)({
            base: 'a',
            local: 'x',
            remote: 'y',
        });
        expect(result.conflicts).toBeGreaterThan(0);
    });
});
//# sourceMappingURL=threeWayMerge.test.js.map