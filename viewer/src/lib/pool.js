// Worker pool with a priority queue: newest view requests first, stale ones cancelled before they start.
import TileWorker from './worker.js?worker';

export class Pool {
  constructor(size = Math.max(2, Math.min(8, (navigator.hardwareConcurrency || 4) - 1))) {
    this.workers = Array.from({ length: size }, () => ({ w: new TileWorker(), busy: false }));
    this.queue = [];
    this.pending = new Map();
    this.nextId = 1;
    for (const slot of this.workers) {
      slot.w.onmessage = (e) => {
        const p = this.pending.get(e.data.id);
        this.pending.delete(e.data.id);
        slot.busy = false;
        slot.cls = null;
        if (p) e.data.error ? p.reject(new Error(e.data.error)) : p.resolve(e.data);
        this.pump();
      };
    }
  }

  /** `priority`: lower runs first. `key` identifies the job for cancellation/dedupe; `ns` namespaces it (prune only
   *  touches its own namespace, so the street view's jobs survive the map's pruning). `cls: 'search'`: a seed
   *  search batch; while any is queued, half the workers (at least one) are kept for searches whatever the map
   *  queues, so a search never waits behind map loading on few-core devices. */
  request(msg, { priority = 0, key = null, ns = null, cls = null } = {}) {
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.queue.push({ id, msg: { ...msg, id }, priority, key, ns, cls, resolve, reject });
      this.pump();
    });
  }

  /** Drop queued (not yet running) jobs whose key fails `keep`. */
  prune(keep, ns = null) {
    this.queue = this.queue.filter((j) => {
      if (j.ns !== ns || j.key === null || keep(j.key)) return true;
      j.reject(new Error('cancelled'));
      return false;
    });
  }

  reprioritize(fn, ns = null) {
    for (const j of this.queue) if (j.ns === ns && j.key !== null) j.priority = fn(j.key, j.priority);
  }

  pump() {
    const reserve = Math.max(1, Math.floor(this.workers.length / 2));
    for (const slot of this.workers) {
      if (slot.busy || this.queue.length === 0) continue;
      const searching = this.workers.filter((w) => w.busy && w.cls === 'search').length;
      // one worker only: alternate search batches with other jobs
      const wantSearch = (this.workers.length > 1 ? searching < reserve : !this.lastSearch) && this.queue.some((j) => j.cls === 'search');
      let bi = -1;
      for (let i = 0; i < this.queue.length; i++) {
        if (wantSearch && this.queue[i].cls !== 'search') continue;
        if (bi < 0 || this.queue[i].priority < this.queue[bi].priority) bi = i;
      }
      const job = this.queue.splice(bi, 1)[0];
      slot.busy = true;
      slot.cls = job.cls;
      this.lastSearch = job.cls === 'search';
      this.pending.set(job.id, job);
      slot.w.postMessage(job.msg);
    }
  }

  get size() {
    return this.workers.length;
  }
}
