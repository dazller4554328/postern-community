/** Shared lightbox state — any screenshot can open a full-size view. */
class Lightbox {
  src = $state<string | null>(null);
  alt = $state('');

  open(src: string, alt = '') {
    this.src = src;
    this.alt = alt;
  }

  close() {
    this.src = null;
    this.alt = '';
  }
}

export const lightbox = new Lightbox();
