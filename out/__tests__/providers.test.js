"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const routingPolicy_1 = require("../core/routingPolicy");
const jobQueue_1 = require("../core/jobQueue");
describe('Provider routing', () => {
    const routing = new routingPolicy_1.RoutingPolicy();
    test('keeps short code tasks on a preferred local provider', () => {
        expect(routing.selectProvider({
            tokenCount: 500,
            taskComplexity: 'simple',
            isCodeTask: true,
            preferLocal: true
        })).toBe('local');
    });
    test('routes large, complex tasks to the cloud', () => {
        expect(routing.selectProvider({
            tokenCount: 20_000,
            taskComplexity: 'complex',
            isCodeTask: true,
            preferLocal: true
        })).toBe('cloud');
    });
    test('selects a recommended model based on task size', () => {
        expect(routing.getRecommendedModel({
            tokenCount: 20_000,
            taskComplexity: 'complex',
            isCodeTask: true
        })).toBe('gpt-4o-mini');
    });
});
describe('JobQueue', () => {
    test('returns handler results and respects configured concurrency', async () => {
        const queue = new jobQueue_1.JobQueue(2);
        let active = 0;
        let peak = 0;
        const handler = async (value) => {
            active += 1;
            peak = Math.max(peak, active);
            await new Promise(resolve => setTimeout(resolve, 5));
            active -= 1;
            return value * 2;
        };
        await expect(Promise.all([
            queue.enqueue(1, handler),
            queue.enqueue(2, handler),
            queue.enqueue(3, handler)
        ])).resolves.toEqual([2, 4, 6]);
        expect(peak).toBe(2);
    });
});
//# sourceMappingURL=providers.test.js.map