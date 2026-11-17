"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const routingPolicy_1 = require("../core/routingPolicy");
const jobQueue_1 = require("../core/jobQueue");
const responseParser_1 = require("../providers/responseParser");
const providerFallback_1 = require("../providers/providerFallback");
const localEndpoint_1 = require("../providers/localEndpoint");
describe('Local OpenAI-compatible endpoints', () => {
    test('normalizes and routes endpoints already ending in /v1', () => {
        expect((0, localEndpoint_1.normalizeLocalEndpoint)(' http://localhost:1234/v1/ ')).toBe('http://localhost:1234/v1');
        expect((0, localEndpoint_1.getLocalChatRoutes)('http://localhost:1234/v1')[0].path).toBe('/chat/completions');
        expect((0, localEndpoint_1.getLocalProbePaths)('http://localhost:1234/v1')).toContain('/models');
    });
    test('routes a host-root endpoint through the OpenAI-compatible v1 path', () => {
        expect((0, localEndpoint_1.getLocalChatRoutes)('http://localhost:9000')[0].path).toBe('/v1/chat/completions');
    });
});
describe('Provider fallback', () => {
    test('uses the next provider after the preferred provider fails', async () => {
        const attempted = [];
        const result = await (0, providerFallback_1.runWithProviderFallback)(['local', 'openai'], async (provider) => {
            attempted.push(provider);
            if (provider === 'local')
                throw new Error('offline');
            return ' cloud answer ';
        });
        expect(attempted).toEqual(['local', 'openai']);
        expect(result.value).toBe('cloud answer');
        expect(result.provider).toBe('openai');
        expect(result.failures).toEqual([{ provider: 'local', error: 'offline' }]);
    });
    test('does not call fallback when the preferred provider succeeds', async () => {
        const attempt = jest.fn(async () => 'local answer');
        const result = await (0, providerFallback_1.runWithProviderFallback)(['local', 'openai'], attempt);
        expect(result.value).toBe('local answer');
        expect(attempt).toHaveBeenCalledTimes(1);
    });
});
describe('OpenAI-compatible response parsing', () => {
    test('parses chat completion string content', () => {
        expect((0, responseParser_1.parseOpenAICompatibleResponse)({
            choices: [{ message: { content: '  Parsed answer  ' } }]
        })).toBe('Parsed answer');
    });
    test('parses multipart text content and ignores non-text parts', () => {
        expect((0, responseParser_1.parseOpenAICompatibleResponse)({
            choices: [{ message: { content: [{ type: 'text', text: 'First ' }, { type: 'image' }, { type: 'text', text: 'second' }] } }]
        })).toBe('First second');
    });
    test('returns an empty result for malformed payloads', () => {
        expect((0, responseParser_1.parseOpenAICompatibleResponse)({ choices: [] })).toBe('');
        expect((0, responseParser_1.parseOpenAICompatibleResponse)(null)).toBe('');
    });
});
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