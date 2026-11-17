export type QueueHandler<T, R = void> = (job: T) => Promise<R>;

export interface QueueTask<T, R = void> {
  job: T;
  handler: QueueHandler<T, R>;
  resolve: (value: R) => void;
  reject: (reason?: unknown) => void;
}

export class JobQueue<T> {
  private readonly queue: Array<QueueTask<T, unknown>> = [];
  private active = 0;

  constructor(private readonly concurrency: number = 1) {
    if (this.concurrency < 1) {
      this.concurrency = 1;
    }
  }

  async enqueue<R = void>(job: T, handler: QueueHandler<T, R>): Promise<R> {
    return new Promise<R>((resolve, reject) => {
      this.queue.push({ job, handler: handler as QueueHandler<T, unknown>, resolve: resolve as (value: unknown) => void, reject });
      void this.drain();
    });
  }

  private async drain(): Promise<void> {
    while (this.active < this.concurrency && this.queue.length > 0) {
      const task = this.queue.shift();
      if (!task) {
        continue;
      }

      this.active += 1;
      try {
        const result = await task.handler(task.job);
        task.resolve(result);
      } catch (error) {
        task.reject(error);
      } finally {
        this.active -= 1;
      }
    }
  }
}

