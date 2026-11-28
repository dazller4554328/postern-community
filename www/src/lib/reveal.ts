/**
 * Svelte action: add `is-visible` when the element scrolls into view.
 * Pairs with the `.reveal` class in global.css. One-shot — unobserves
 * after the first intersection. Respects reduced-motion by revealing
 * immediately.
 */
export function reveal(node: HTMLElement, delay = 0) {
  if (typeof IntersectionObserver === 'undefined') {
    node.classList.add('is-visible');
    return;
  }

  if (delay) node.style.transitionDelay = `${delay}ms`;

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          node.classList.add('is-visible');
          observer.unobserve(node);
        }
      }
    },
    { threshold: 0.12, rootMargin: '0px 0px -8% 0px' }
  );

  observer.observe(node);

  return {
    destroy() {
      observer.disconnect();
    }
  };
}
