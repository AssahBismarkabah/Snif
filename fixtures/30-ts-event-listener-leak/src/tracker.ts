export class ScrollTracker {
  private count = 0;

  start(): void {
    window.addEventListener("scroll", this.onScroll.bind(this));
  }

  private onScroll(): void {
    this.count++;
  }
}

export class ScrollTrackerSafe {
  private count = 0;
  private handler: (() => void) | null = null;

  start(): void {
    this.handler = this.onScroll.bind(this);
    window.addEventListener("scroll", this.handler);
  }

  stop(): void {
    if (this.handler) {
      window.removeEventListener("scroll", this.handler);
      this.handler = null;
    }
  }

  private onScroll(): void {
    this.count++;
  }
}
