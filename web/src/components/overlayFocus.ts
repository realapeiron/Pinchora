const focusable =
  "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

export function trapTab(
  event: { key: string; shiftKey: boolean; preventDefault: () => void },
  container: HTMLElement,
) {
  if (event.key !== "Tab") return;
  const items = [...container.querySelectorAll<HTMLElement>(focusable)].filter(
    (item) =>
      item.tabIndex >= 0 &&
      !item.closest("[inert]") &&
      item.getClientRects().length > 0 &&
      getComputedStyle(item).visibility !== "hidden",
  );
  const first = items[0];
  const last = items.at(-1);
  if (!first || !last) return;
  if (event.shiftKey && (document.activeElement === first || !container.contains(document.activeElement))) {
    event.preventDefault();
    last.focus();
  } else if (
    !event.shiftKey &&
    (document.activeElement === last || !container.contains(document.activeElement))
  ) {
    event.preventDefault();
    first.focus();
  }
}
