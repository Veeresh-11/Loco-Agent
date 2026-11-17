"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JobQueue = void 0;
class JobQueue {
    concurrency;
    queue = [];
    active = 0;
    constructor(concurrency = 1) {
        this.concurrency = concurrency;
        if (this.concurrency < 1) {
            this.concurrency = 1;
        }
    }
    async enqueue(job, handler) {
        return new Promise((resolve, reject) => {
            this.queue.push({ job, handler: handler, resolve: resolve, reject });
            void this.drain();
        });
    }
    async drain() {
        while (this.active < this.concurrency && this.queue.length > 0) {
            const task = this.queue.shift();
            if (!task) {
                continue;
            }
            this.active += 1;
            try {
                const result = await task.handler(task.job);
                task.resolve(result);
            }
            catch (error) {
                task.reject(error);
            }
            finally {
                this.active -= 1;
            }
        }
    }
}
exports.JobQueue = JobQueue;
//# sourceMappingURL=jobQueue.js.map